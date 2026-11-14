//! The gallery: looks, styles and widgets other people have shared.
//!
//! A gallery is a folder (on the web or on this PC) with a `gallery.json`
//! index and the shared files beside it:
//!
//! ```json
//! {
//!   "lumenGallery": 1,
//!   "name": "Lumen gallery",
//!   "items": [
//!     { "id": "dice", "kind": "widget", "name": "Dice", "author": "Someone",
//!       "description": "Rolls when clicked.", "file": "widgets/dice.lumenwidget",
//!       "script": true, "permissions": ["files"] }
//!   ]
//! }
//! ```
//!
//! What the index says about a script is only for showing in the list. What
//! a widget's script really asks for is read from the downloaded file itself
//! before anything is installed.

use serde::{Deserialize, Serialize};

use crate::widget_file::Permissions;

/// The most items a gallery may list.
pub const MAX_ITEMS: usize = 500;

/// What kind of thing a gallery item is; each has its own file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GalleryKind {
    /// A `.lumenwidget` package.
    Widget,
    /// A `.lumenstyle` component style.
    Style,
    /// A `.lumen` preset: a whole look.
    Preset,
}

impl GalleryKind {
    /// The file extension an item of this kind must have.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Widget => "lumenwidget",
            Self::Style => "lumenstyle",
            Self::Preset => "lumen",
        }
    }
}

/// One thing in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub id: String,
    pub kind: GalleryKind,
    pub name: String,
    pub author: String,
    pub description: String,
    /// Where the file is, relative to the index.
    pub file: String,
    /// Whether the index says this widget has a script, and what it says the script asks for.
    pub script: bool,
    pub permissions: Permissions,
}

/// The gallery as the settings app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryState {
    /// Where the gallery is read from: a web address or a folder. `None` until one is set.
    pub source: Option<String>,
    pub name: String,
    pub items: Vec<GalleryItem>,
    /// Why the gallery could not be read, if it could not.
    pub error: Option<String>,
    /// The address of the online gallery everyone shares.
    #[serde(default)]
    pub online: String,
    /// True while the gallery showing is the one that came with Lumen (nothing went online).
    #[serde(default)]
    pub local: bool,
}

/// What `offerToGallery` made: the file to attach, and the page to attach it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryOffer {
    pub path: String,
    pub page: String,
}

/// What happened when an item was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryInstall {
    /// True once the item is in Lumen.
    pub installed: bool,
    /// True if nothing was installed because the widget has a script and the
    /// user has not yet said to go ahead. `script` and `permissions` then say what it is.
    pub needs_ok: bool,
    /// The widget's script file, if it has one (read from the downloaded file).
    pub script: Option<String>,
    pub permissions: Permissions,
    /// What it is called in Lumen now (a widget file's name, a style's or a preset's).
    pub added: String,
}

/// Whether a path inside a gallery is a plain relative one: folders and a
/// file name, nothing that climbs out or points elsewhere.
pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && !path.starts_with('/')
        && path.split('/').all(|piece| !piece.is_empty() && piece != "." && piece != ".." && piece.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
}

fn text_of(value: &serde_json::Value, key: &str, limit: usize) -> String {
    value.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().trim().chars().filter(|c| !c.is_control()).take(limit).collect()
}

/// Reads a `gallery.json`. Returns the gallery's name and its items. An item
/// that is not written properly is left out rather than spoiling the list.
pub fn parse_index(text: &str) -> Result<(String, Vec<GalleryItem>), String> {
    let doc: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| format!("its gallery.json is not valid JSON: {error}"))?;
    if doc.get("lumenGallery").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("its gallery.json does not say it is a Lumen gallery (`\"lumenGallery\": 1`)".into());
    }
    let listed = doc.get("items").and_then(serde_json::Value::as_array).ok_or("its gallery.json has no list of items")?;
    let mut items: Vec<GalleryItem> = Vec::new();
    for raw in listed.iter().take(MAX_ITEMS) {
        let Some(kind) = raw.get("kind").cloned().and_then(|kind| serde_json::from_value::<GalleryKind>(kind).ok()) else { continue };
        let (id, name, file) = (text_of(raw, "id", 64), text_of(raw, "name", 60), text_of(raw, "file", 200));
        let id_ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let file_ok = is_safe_path(&file) && file.rsplit_once('.').is_some_and(|(_, extension)| extension == kind.extension());
        if !id_ok || name.is_empty() || !file_ok || items.iter().any(|other| other.id == id) {
            continue;
        }
        let mut permissions = Permissions::default();
        for asked in raw.get("permissions").and_then(serde_json::Value::as_array).into_iter().flatten().filter_map(serde_json::Value::as_str) {
            match asked {
                "files" => permissions.files = true,
                "network" => permissions.network = true,
                "run" => permissions.run = true,
                _ => {}
            }
        }
        let script = kind == GalleryKind::Widget && (raw.get("script").and_then(serde_json::Value::as_bool).unwrap_or(false) || permissions != Permissions::default());
        items.push(GalleryItem { id, kind, name, author: text_of(raw, "author", 60), description: text_of(raw, "description", 300), file, script, permissions: if script { permissions } else { Permissions::default() } });
    }
    Ok((text_of(&doc, "name", 60), items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_is_read_and_badly_written_items_are_left_out() {
        let text = r#"{ "lumenGallery": 1, "name": "Test gallery", "items": [
            { "id": "dice", "kind": "widget", "name": "Dice", "author": "A", "description": "Rolls.", "file": "widgets/dice.lumenwidget", "script": true, "permissions": ["files", "teleport"] },
            { "id": "neon", "kind": "style", "name": "Neon", "file": "styles/neon.lumenstyle", "permissions": ["run"] },
            { "id": "dawn", "kind": "preset", "name": "Dawn", "file": "dawn.lumen" },
            { "id": "dice", "kind": "widget", "name": "Second dice", "file": "widgets/dice2.lumenwidget" },
            { "id": "climb", "kind": "widget", "name": "Climb", "file": "../outside.lumenwidget" },
            { "id": "web", "kind": "widget", "name": "Web", "file": "https://elsewhere.example/x.lumenwidget" },
            { "id": "wrong", "kind": "widget", "name": "Wrong type", "file": "thing.exe" },
            { "id": "mismatch", "kind": "style", "name": "Mismatch", "file": "a.lumenwidget" },
            { "id": "odd kind", "kind": "virus", "name": "X", "file": "x.lumen" },
            { "id": "", "kind": "preset", "name": "No id", "file": "x.lumen" }
        ] }"#;
        let (name, items) = parse_index(text).unwrap();
        assert_eq!(name, "Test gallery");
        assert_eq!(items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["dice", "neon", "dawn"]);
        assert!(items[0].script && items[0].permissions == Permissions { files: true, network: false, run: false });
        assert!(!items[1].script && items[1].permissions == Permissions::default(), "only a widget can have a script");
        assert_eq!((items[2].kind, items[2].author.as_str()), (GalleryKind::Preset, ""));
    }

    #[test]
    fn things_that_are_not_a_gallery_are_refused() {
        assert!(parse_index("<html>").unwrap_err().contains("not valid JSON"));
        assert!(parse_index("{}").unwrap_err().contains("does not say it is a Lumen gallery"));
        assert!(parse_index(r#"{ "lumenGallery": 1 }"#).unwrap_err().contains("no list of items"));
        assert_eq!(parse_index(r#"{ "lumenGallery": 1, "items": [] }"#).unwrap(), (String::new(), Vec::new()));
    }

    #[test]
    fn only_plain_relative_paths_are_safe() {
        for good in ["a.lumen", "widgets/dice.lumenwidget", "a/b/c_d-e.lumenstyle"] {
            assert!(is_safe_path(good), "{good}");
        }
        for bad in ["", "/a.lumen", "../a.lumen", "a/../b.lumen", "a//b.lumen", "C:/x.lumen", "a\\b.lumen", "a b.lumen", "https://x/y.lumen", "a/./b.lumen", "a?x=1"] {
            assert!(!is_safe_path(bad), "{bad}");
        }
    }
}
