//! Everything Lumen keeps on disk, under `%APPDATA%\Lumen`:
//!
//! ```text
//! config.json            which theme is active
//! themes/<id>.json       theme files
//! wallpapers/<id>.<ext>  imported images
//! wallpapers/index.json  the library, with each wallpaper's extracted colours
//! presets/<id>.json      saved looks
//! widgets/               widget packages (later milestones)
//! ```

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use lumen_ipc::{WallpaperEntry, WallpaperKind};
use lumen_theme::{Color, Theme};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const FOLDERS: [&str; 4] = ["themes", "presets", "wallpapers", "widgets"];
const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "bmp"];

/// What can go wrong reading or writing Lumen's files.
#[derive(Debug)]
pub enum StoreError {
    /// The thing asked for does not exist.
    NotFound(String),
    /// It exists but cannot be used (not an image, broken JSON, ...).
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) | Self::Invalid(m) | Self::Io(m) => f.write_str(m),
        }
    }
}

type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    /// Id of the kept theme (`themes/<id>.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_theme: Option<String>,
}

/// A library entry as stored in `wallpapers/index.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWallpaper {
    id: String,
    name: String,
    file: String,
    kind: WallpaperKind,
    palette: Vec<Color>,
    added_at: u64,
    /// Content hash, so importing the same image twice returns the first copy.
    hash: String,
}

/// A saved look (`presets/<id>.json`). The widget and bar layouts are carried
/// as-is until the milestones that define them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    /// Theme file, relative to the data folder (`themes/x.json`).
    pub theme: String,
    /// Wallpaper per monitor: key is a monitor id, or `"all"`; value is a path
    /// relative to the data folder or absolute.
    #[serde(default)]
    pub wallpapers: BTreeMap<String, String>,
    #[serde(default)]
    pub widgets: Value,
    #[serde(default)]
    pub bar: Value,
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// `LUMEN_DATA_DIR` if set (tests, clean-profile checks), else `%APPDATA%\Lumen`.
    pub fn default_root() -> PathBuf {
        if let Some(dir) = std::env::var_os("LUMEN_DATA_DIR").filter(|d| !d.is_empty()) {
            return PathBuf::from(dir);
        }
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        appdata.join("Lumen")
    }

    /// Opens the data folder, creating it and its sub-folders if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        for folder in FOLDERS {
            fs::create_dir_all(root.join(folder))?;
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Turns a stored path (relative to the data folder, or absolute) into a real one.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() { path.to_owned() } else { self.root.join(path) }
    }

    // ----- themes -----

    /// The kept theme, or `None` on a fresh install.
    pub fn active_theme(&self) -> Option<Theme> {
        let config: Config = self.read_json("config.json").ok()?;
        self.read_json(&format!("themes/{}.json", config.active_theme?)).ok()
    }

    /// Saves a theme and makes it the active one.
    pub fn save_active_theme(&self, theme: &Theme) -> Result<()> {
        let id = slug(&theme.name);
        self.write_json(&format!("themes/{id}.json"), theme)?;
        let mut config: Config = self.read_json("config.json").unwrap_or_default();
        config.active_theme = Some(id);
        self.write_json("config.json", &config)
    }

    pub fn theme_file(&self, relative: &str) -> Result<Theme> {
        self.read_json(relative)
    }

    // ----- wallpapers -----

    pub fn wallpapers(&self) -> Vec<WallpaperEntry> {
        self.index().into_iter().map(|w| self.entry(w)).collect()
    }

    /// The library entry for a stored path such as `wallpapers/x.jpg`, if it is one.
    pub fn wallpaper_by_file(&self, file: &str) -> Option<WallpaperEntry> {
        let wanted = self.resolve(file);
        self.wallpapers().into_iter().find(|w| Path::new(&w.path) == wanted)
    }

    /// Copies an image into the library and records its colours.
    /// Importing the same image again returns the existing entry.
    pub fn import_wallpaper(&self, source: &Path) -> Result<WallpaperEntry> {
        let shown = source.display();
        if !source.is_file() {
            return Err(StoreError::NotFound(format!("no such file: {shown}")));
        }
        let extension = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
            return Err(StoreError::Invalid(format!("{shown} is not a supported image (jpg, png, webp, bmp)")));
        }
        let bytes = fs::read(source).map_err(|e| StoreError::Io(format!("could not read {shown}: {e}")))?;
        let hash = format!("{:016x}", fnv1a(&bytes));

        let mut index = self.index();
        if let Some(existing) = index.iter().find(|w| w.hash == hash && self.resolve(&w.file).is_file()) {
            return Ok(self.entry(existing.clone()));
        }

        // Extracting the colours also proves the file really is an image.
        let colors = lumen_theme::extract(source).map_err(|e| StoreError::Invalid(e.to_string()))?;

        let base = slug(&source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        let id = unique(&base, |candidate| index.iter().any(|w| w.id == candidate));
        let file = format!("wallpapers/{id}.{extension}");
        fs::write(self.root.join(&file), &bytes).map_err(|e| StoreError::Io(format!("could not copy {shown} into the library: {e}")))?;

        let stored = StoredWallpaper {
            name: display_name(&id),
            id,
            file,
            kind: WallpaperKind::Static,
            palette: colors.candidates,
            added_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            hash,
        };
        index.push(stored.clone());
        self.write_json("wallpapers/index.json", &index)?;
        Ok(self.entry(stored))
    }

    fn index(&self) -> Vec<StoredWallpaper> {
        self.read_json("wallpapers/index.json").unwrap_or_default()
    }

    fn entry(&self, w: StoredWallpaper) -> WallpaperEntry {
        WallpaperEntry {
            path: self.resolve(&w.file).display().to_string(),
            id: w.id,
            name: w.name,
            file: w.file,
            kind: w.kind,
            palette: w.palette,
            added_at: w.added_at,
        }
    }

    // ----- presets -----

    pub fn preset(&self, id: &str) -> Result<Preset> {
        // Ids are file names; never let one walk out of the presets folder.
        if id.is_empty() || id.contains(['/', '\\', ':']) || id.contains("..") {
            return Err(StoreError::Invalid(format!("\"{id}\" is not a preset id")));
        }
        self.read_json(&format!("presets/{id}.json"))
    }

    // ----- files -----

    fn read_json<T: DeserializeOwned>(&self, relative: &str) -> Result<T> {
        let path = self.root.join(relative);
        let text = fs::read_to_string(&path).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => StoreError::NotFound(format!("{relative} does not exist")),
            _ => StoreError::Io(format!("could not read {relative}: {e}")),
        })?;
        serde_json::from_str(&text).map_err(|e| StoreError::Invalid(format!("{relative} is not valid: {e}")))
    }

    /// Writes via a temporary file so a crash never leaves a half-written file.
    fn write_json<T: Serialize>(&self, relative: &str, value: &T) -> Result<()> {
        let path = self.root.join(relative);
        let temp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(value).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::write(&temp, text).and_then(|()| fs::rename(&temp, &path)).map_err(|e| StoreError::Io(format!("could not write {relative}: {e}")))
    }
}

/// `Midnight Harbour!` -> `midnight-harbour`. Always a safe file name.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() { "untitled".to_owned() } else { out.to_owned() }
}

/// `midnight-harbour-2` -> `Midnight Harbour 2`.
fn display_name(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `base`, or `base-2`, `base-3`, ... whichever is not taken yet.
fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|candidate| !taken(candidate)).expect("an unused suffix always exists")
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use lumen_theme::{Mode, Variant};

    use super::*;

    /// A fresh, empty data folder under the system temp folder.
    pub(crate) fn temp_store() -> Store {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("lumen-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&dir);
        Store::open(dir).unwrap()
    }

    /// Writes a small two-colour PNG and returns its path.
    pub(crate) fn test_image(dir: &Path, name: &str, colour: [u8; 3]) -> PathBuf {
        let path = dir.join(name);
        image::RgbImage::from_fn(64, 64, |x, _| if x < 48 { image::Rgb(colour) } else { image::Rgb([240, 200, 140]) }).save(&path).unwrap();
        path
    }

    #[test]
    fn open_creates_the_folders() {
        let store = temp_store();
        for folder in FOLDERS {
            assert!(store.root().join(folder).is_dir(), "{folder}");
        }
        assert!(store.active_theme().is_none());
        assert!(store.wallpapers().is_empty());
    }

    #[test]
    fn active_theme_survives_reopening() {
        let store = temp_store();
        let mut theme = Theme::from_color(Color::new(0x2F, 0x5B, 0xEA), Variant::Tonal, Mode::Dark, 0).unwrap();
        theme.name = "Midnight Harbour".into();
        store.save_active_theme(&theme).unwrap();
        assert!(store.root().join("themes/midnight-harbour.json").is_file());

        let reopened = Store::open(store.root()).unwrap();
        assert_eq!(reopened.active_theme(), Some(theme));
    }

    #[test]
    fn import_copies_the_image_and_keeps_its_palette() {
        let store = temp_store();
        let source = test_image(store.root(), "Forest Day.png", [40, 140, 70]);
        let entry = store.import_wallpaper(&source).unwrap();

        assert_eq!(entry.id, "forest-day");
        assert_eq!(entry.name, "Forest Day");
        assert_eq!(entry.file, "wallpapers/forest-day.png");
        assert!(Path::new(&entry.path).is_file());
        assert!(!entry.palette.is_empty());
        assert_eq!(store.wallpapers(), vec![entry.clone()]);
        assert_eq!(store.wallpaper_by_file("wallpapers/forest-day.png"), Some(entry));
    }

    #[test]
    fn importing_the_same_image_twice_keeps_one_copy() {
        let store = temp_store();
        let source = test_image(store.root(), "a.png", [200, 40, 40]);
        let first = store.import_wallpaper(&source).unwrap();
        let again = store.import_wallpaper(&source).unwrap();
        assert_eq!(first, again);
        assert_eq!(store.wallpapers().len(), 1);
    }

    #[test]
    fn different_images_with_the_same_name_get_different_ids() {
        let store = temp_store();
        let dir = store.root().join("elsewhere");
        fs::create_dir_all(&dir).unwrap();
        let first = store.import_wallpaper(&test_image(store.root(), "sky.png", [40, 80, 200])).unwrap();
        let second = store.import_wallpaper(&test_image(&dir, "sky.png", [200, 80, 40])).unwrap();
        assert_eq!((first.id.as_str(), second.id.as_str()), ("sky", "sky-2"));
    }

    #[test]
    fn import_rejects_missing_and_non_image_files() {
        let store = temp_store();
        assert!(matches!(store.import_wallpaper(Path::new("nope.png")), Err(StoreError::NotFound(_))));

        let text = store.root().join("notes.txt");
        fs::write(&text, "hello").unwrap();
        assert!(matches!(store.import_wallpaper(&text), Err(StoreError::Invalid(_))));

        let fake = store.root().join("fake.png");
        fs::write(&fake, "not really a png").unwrap();
        assert!(matches!(store.import_wallpaper(&fake), Err(StoreError::Invalid(_))));
        assert!(store.wallpapers().is_empty());
    }

    #[test]
    fn preset_ids_cannot_leave_the_presets_folder() {
        let store = temp_store();
        for bad in ["", "../config", r"..\config", "a/b", "C:evil"] {
            assert!(matches!(store.preset(bad), Err(StoreError::Invalid(_))), "{bad:?}");
        }
        assert!(matches!(store.preset("missing"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("Midnight Harbour"), "midnight-harbour");
        assert_eq!(slug("  ../..\\evil:name?  "), "evil-name");
        assert_eq!(slug("#8EA6FF"), "8ea6ff");
        assert_eq!(slug("!!!"), "untitled");
    }
}
