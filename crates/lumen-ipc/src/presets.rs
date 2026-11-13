//! Presets: a saved look (theme + wallpaper per monitor + widget layout),
//! and the `.lumen` file that carries one between PCs.

use lumen_theme::{Color, Theme};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::widgets::Layout;

/// The version written into `.lumen` files and preset files.
pub const PRESET_FORMAT: u32 = 1;
/// The file extension of an exported preset, without the dot.
pub const PRESET_EXTENSION: &str = "lumen";

/// A preset as shown in lists (the Presets screen, Quick switch, the tray menu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    /// Absolute path of a small preview image (its first wallpaper), if it has one.
    pub thumb: Option<String>,
    /// Five colours that sum up its theme.
    pub palette: Vec<Color>,
    /// Seconds since 1970 (UTC).
    pub created_at: u64,
    /// When it was last applied; 0 if never.
    pub last_used_at: u64,
    pub widget_count: usize,
    /// True for the preset that was applied or saved most recently, until
    /// the look is changed by hand.
    pub active: bool,
}

/// One wallpaper in a preset. Monitors are identified by number (1 = leftmost),
/// not by hardware id, so a preset means the same thing on another PC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetWallpaper {
    pub monitor: usize,
    /// Relative to the data folder (or to the root of a `.lumen` file).
    pub file: String,
}

/// A saved look, as stored in `presets/<id>.json`.
///
/// Inside the widget layout, a placement's `monitor` is `"#2"`-style (a
/// monitor number) rather than a hardware id; absent still means the main monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    #[serde(default = "current_format")]
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_used_at: u64,
    pub theme: Theme,
    #[serde(default)]
    pub wallpapers: Vec<PresetWallpaper>,
    /// The widget layout; `None` leaves the widgets as they are when applied.
    #[serde(default)]
    pub widgets: Option<Layout>,
    /// The bar's settings ([`BarConfig`](crate::BarConfig)); null in presets saved before the bar existed.
    #[serde(default)]
    pub bar: Value,
    /// The flyout settings ([`FlyoutConfig`](crate::FlyoutConfig)); null in presets saved before flyouts existed.
    #[serde(default)]
    pub flyouts: Value,
    /// The launcher's settings ([`LauncherConfig`](crate::LauncherConfig)); null in presets saved before the launcher existed.
    #[serde(default)]
    pub launcher: Value,
}

fn current_format() -> u32 {
    PRESET_FORMAT
}

/// `preset.json` inside a `.lumen` file: the same as [`Preset`], except the
/// theme is a separate file in the archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetManifest {
    pub format_version: u32,
    pub name: String,
    /// Path of the theme file inside the archive (`theme.json`).
    pub theme: String,
    #[serde(default)]
    pub wallpapers: Vec<PresetWallpaper>,
    #[serde(default)]
    pub widgets: Option<Layout>,
    #[serde(default)]
    pub bar: Value,
    #[serde(default)]
    pub flyouts: Value,
    #[serde(default)]
    pub launcher: Value,
}

/// Params of `savePreset`: save the look that is on the desktop now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePresetParams {
    pub name: String,
}

/// Params of `applyPreset`, `duplicatePreset` and `deletePreset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetIdParams {
    pub id: String,
}

/// Params of `renamePreset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenamePresetParams {
    pub id: String,
    pub name: String,
}

/// Params of `exportPreset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportPresetParams {
    pub id: String,
    /// Where to write the `.lumen` file (absolute path).
    pub path: String,
}

/// Params of `importPreset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportPresetParams {
    /// The `.lumen` file to read (absolute path).
    pub path: String,
}

/// The monitor reference used inside presets for monitor `number`: `"#2"`.
pub fn monitor_ref(number: usize) -> String {
    format!("#{number}")
}

/// Reads a `"#2"`-style reference back into a monitor number.
pub fn monitor_number(reference: &str) -> Option<usize> {
    reference.strip_prefix('#')?.parse().ok().filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_references_round_trip() {
        assert_eq!(monitor_ref(2), "#2");
        assert_eq!(monitor_number("#2"), Some(2));
        for not_a_reference in ["2", "#", "#0", "#two", r"\\?\DISPLAY#GSM1234"] {
            assert_eq!(monitor_number(not_a_reference), None, "{not_a_reference}");
        }
    }

    #[test]
    fn a_minimal_preset_file_loads_with_defaults() {
        let theme = serde_json::to_value(Theme::fallback()).unwrap();
        let preset: Preset = serde_json::from_value(serde_json::json!({ "name": "Bare", "theme": theme })).unwrap();
        assert_eq!(preset.format_version, PRESET_FORMAT);
        assert!(preset.wallpapers.is_empty() && preset.widgets.is_none() && preset.bar.is_null());
        assert_eq!(preset.last_used_at, 0);
    }
}
