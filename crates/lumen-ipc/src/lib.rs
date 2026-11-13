//! Shared message types for `lumen-core` <-> clients (the settings app, `lumen-ctl`).
//!
//! Transport: the named pipe [`PIPE_NAME`], carrying JSON-RPC 2.0, one JSON
//! object per line. Clients send requests; the core answers each one and also
//! pushes notifications ([`event`]) to every connected client.

use lumen_theme::{Color, Mode, Source, Style, Theme, Variant};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod presets;
pub mod widgets;

pub use presets::{ExportPresetParams, ImportPresetParams, Preset, PresetIdParams, PresetInfo, PresetManifest, PresetWallpaper, RenamePresetParams, SavePresetParams};
pub use widgets::{
    AddWidgetParams, Background, CostStatus, EditModeParams, WidgetCost, Guides, Layout, LayoutOptionsParams, MonitorArea, Placement, SelectWidgetParams, UpdateWidgetParams, WidgetIdParams,
    WidgetKind, WidgetStyle, WidgetsState,
};

/// The core's pipe. Override with the `LUMEN_PIPE` environment variable (tests).
pub const PIPE_NAME: &str = r"\\.\pipe\lumen-core";

/// How long a preview stays on the desktop before it reverts by itself.
pub const PREVIEW_SECONDS: u64 = 15;

/// The pipe name to use: `LUMEN_PIPE` if set, otherwise [`PIPE_NAME`].
pub fn pipe_name() -> String {
    std::env::var("LUMEN_PIPE").ok().filter(|name| !name.is_empty()).unwrap_or_else(|| PIPE_NAME.to_owned())
}

/// Request method names.
pub mod method {
    /// No params. Result: [`ThemeState`](super::ThemeState).
    pub const GET_THEME: &str = "getTheme";
    /// Params: [`PreviewThemeParams`](super::PreviewThemeParams). Result: [`PreviewState`](super::PreviewState).
    pub const PREVIEW_THEME: &str = "previewTheme";
    /// No params. Keeps the preview. Result: the committed `Theme`.
    pub const COMMIT_PREVIEW: &str = "commitPreview";
    /// No params. Puts the desktop back. Result: [`ThemeState`](super::ThemeState).
    pub const REVERT_PREVIEW: &str = "revertPreview";
    /// No params. Result: array of [`WallpaperEntry`](super::WallpaperEntry).
    pub const LIST_WALLPAPERS: &str = "listWallpapers";
    /// Params: [`ImportWallpaperParams`](super::ImportWallpaperParams). Result: [`WallpaperEntry`](super::WallpaperEntry).
    pub const IMPORT_WALLPAPER: &str = "importWallpaper";
    /// Params: [`PresetIdParams`](super::PresetIdParams). Applies at once: wallpapers,
    /// theme and widget layout. Result: the applied `Theme`.
    pub const APPLY_PRESET: &str = "applyPreset";
    /// No params. Result: array of [`PresetInfo`](super::PresetInfo), most recently used first.
    pub const LIST_PRESETS: &str = "listPresets";
    /// Params: [`SavePresetParams`](super::SavePresetParams). Saves the look on the desktop now.
    /// Result: [`PresetInfo`](super::PresetInfo).
    pub const SAVE_PRESET: &str = "savePreset";
    /// Params: [`RenamePresetParams`](super::RenamePresetParams). Result: [`PresetInfo`](super::PresetInfo).
    pub const RENAME_PRESET: &str = "renamePreset";
    /// Params: [`PresetIdParams`](super::PresetIdParams). Result: the copy's [`PresetInfo`](super::PresetInfo).
    pub const DUPLICATE_PRESET: &str = "duplicatePreset";
    /// Params: [`PresetIdParams`](super::PresetIdParams). Result: `null`.
    pub const DELETE_PRESET: &str = "deletePreset";
    /// Params: [`ExportPresetParams`](super::ExportPresetParams). Writes a `.lumen` file. Result: `null`.
    pub const EXPORT_PRESET: &str = "exportPreset";
    /// Params: [`ImportPresetParams`](super::ImportPresetParams). Reads a `.lumen` file.
    /// Result: [`PresetInfo`](super::PresetInfo).
    pub const IMPORT_PRESET: &str = "importPreset";
    /// No params. Result: [`Status`](super::Status).
    pub const GET_STATUS: &str = "getStatus";
    /// No params. Result: array of [`MonitorInfo`](super::MonitorInfo).
    pub const LIST_MONITORS: &str = "listMonitors";
    /// Params: [`SetWallpaperParams`](super::SetWallpaperParams). Applies at once, without a preview. Result: `null`.
    pub const SET_WALLPAPER: &str = "setWallpaper";
    /// Params: [`UpdateWallpaperParams`](super::UpdateWallpaperParams). Result: [`WallpaperEntry`](super::WallpaperEntry).
    pub const UPDATE_WALLPAPER: &str = "updateWallpaper";
    /// No params. Result: [`WidgetsState`](super::WidgetsState). The widget methods below
    /// all answer with the state as it was when asked; the change itself
    /// arrives in a `widgetsChanged` event.
    pub const GET_WIDGETS: &str = "getWidgets";
    /// Params: [`UpdateWidgetParams`](super::UpdateWidgetParams).
    pub const UPDATE_WIDGET: &str = "updateWidget";
    /// Params: [`AddWidgetParams`](super::AddWidgetParams).
    pub const ADD_WIDGET: &str = "addWidget";
    /// Params: [`WidgetIdParams`](super::WidgetIdParams).
    pub const REMOVE_WIDGET: &str = "removeWidget";
    /// Params: [`SelectWidgetParams`](super::SelectWidgetParams).
    pub const SELECT_WIDGET: &str = "selectWidget";
    /// Params: [`LayoutOptionsParams`](super::LayoutOptionsParams).
    pub const SET_LAYOUT_OPTIONS: &str = "setLayoutOptions";
    /// Params: [`EditModeParams`](super::EditModeParams).
    pub const SET_EDIT_MODE: &str = "setEditMode";
    /// No params. Result: array of [`WidgetCost`](super::WidgetCost), as last measured
    /// (about every ten seconds while widgets are running).
    pub const GET_WIDGET_COSTS: &str = "getWidgetCosts";
    /// Params: [`WidgetIdParams`](super::WidgetIdParams). Lets a widget the governor
    /// switched off run again.
    pub const RESUME_WIDGET: &str = "resumeWidget";
}

/// Notification names pushed by the core.
pub mod event {
    /// Params: [`ThemeChanged`](super::ThemeChanged).
    pub const THEME_CHANGED: &str = "themeChanged";
    /// Params: [`GameModeChanged`](super::GameModeChanged).
    pub const GAME_MODE_CHANGED: &str = "gameModeChanged";
    /// Params: [`WidgetsState`](super::WidgetsState).
    pub const WIDGETS_CHANGED: &str = "widgetsChanged";
    /// Params: array of [`PresetInfo`](super::PresetInfo) (the whole list).
    pub const PRESETS_CHANGED: &str = "presetsChanged";
}

/// JSON-RPC error codes: the standard ones plus Lumen's own (positive).
pub mod code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    /// `commitPreview` with nothing being previewed.
    pub const NO_PREVIEW: i32 = 1;
    /// A wallpaper, preset or file that does not exist.
    pub const NOT_FOUND: i32 = 2;
    /// Windows refused a desktop change (wallpaper, mode or accent).
    pub const DESKTOP_FAILED: i32 = 3;
    /// The desktop widgets are not running.
    pub const WIDGETS_UNAVAILABLE: i32 = 4;
}

// ---------- JSON-RPC envelope ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// Absent for notifications, which get no reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id: Some(id.into()), method: method.into(), params }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn err(id: Value, error: RpcError) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None, error: Some(error) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

/// A server-to-client event (a request without an `id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl Notification {
    pub fn new(method: &str, params: impl Serialize) -> Self {
        Self { jsonrpc: "2.0".into(), method: method.into(), params: serde_json::to_value(params).unwrap_or(Value::Null) }
    }
}

// ---------- Payloads ----------

/// Result of `getTheme` and `revertPreview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeState {
    /// The kept theme; `null` until the first theme is committed.
    pub theme: Option<Theme>,
    /// The theme being tried on the desktop right now, if any.
    pub preview: Option<PreviewState>,
}

/// A theme that is on the desktop but not yet kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewState {
    pub theme: Theme,
    /// Seconds until it reverts by itself.
    pub seconds_left: u64,
}

/// Params of `previewTheme`. Either give a complete `theme`, or let the core
/// generate one: any field left out is taken from the current theme, so
/// `{ "variant": "vibrant" }` alone re-generates the current look as Vibrant.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewThemeParams {
    /// Use exactly this theme (manual and fine-tuned colours).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// Generate from this wallpaper or colour. A wallpaper path may be relative
    /// to the data folder (`wallpapers/x.jpg`) or absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Style>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_windows_accent: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Put the source wallpaper on this monitor only (a monitor id); all monitors if left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper_monitor: Option<String>,
}

/// Why the theme changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeChangeReason {
    Preview,
    Commit,
    Revert,
    /// The 15-second preview timer ran out.
    AutoRevert,
    Preset,
}

/// Params of the `themeChanged` event: the theme every surface should now show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeChanged {
    pub theme: Option<Theme>,
    /// True while this theme is only being previewed.
    pub preview: bool,
    pub reason: ThemeChangeReason,
}

/// Params of the `gameModeChanged` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameModeChanged {
    /// True while a fullscreen app or game is in front.
    pub active: bool,
    /// Its window title.
    pub app: Option<String>,
}

/// One wallpaper in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallpaperEntry {
    pub id: String,
    pub name: String,
    /// Path relative to the data folder, e.g. `wallpapers/midnight-harbour.jpg`.
    pub file: String,
    /// Absolute path on this PC.
    pub path: String,
    /// Absolute path of a small JPEG preview (about 640 px wide).
    pub thumb: String,
    pub kind: WallpaperKind,
    /// Colours extracted at import, best theme seed first.
    pub palette: Vec<Color>,
    /// Seconds since 1970 (UTC).
    pub added_at: u64,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WallpaperKind {
    Static,
    Animated,
}

/// Params of `importWallpaper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportWallpaperParams {
    /// Absolute path of the image to copy into the library.
    pub path: String,
}


/// Result of `getStatus`: what the Home status strip shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    /// The data folder in use (`%APPDATA%\Lumen` unless overridden).
    pub data_dir: String,
    /// The core's memory in use (working set), in megabytes.
    pub memory_mb: f64,
    /// The core's share of the whole CPU since the previous `getStatus`, 0–100.
    pub cpu_percent: f64,
    pub game_mode: GameModeChanged,
    /// True while a preview is on the desktop.
    pub previewing: bool,
}

/// One connected monitor, as returned by `listMonitors`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// 1-based, left to right then top to bottom.
    pub number: usize,
    /// Windows' device path; the stable identifier to store and send back.
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
    /// Absolute path of the image it shows now, if any.
    pub wallpaper: Option<String>,
}

/// Params of `setWallpaper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetWallpaperParams {
    /// Relative to the data folder (`wallpapers/x.jpg`) or absolute.
    pub file: String,
    /// A monitor id; every monitor if left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

/// Params of `updateWallpaper`. Only the fields given are changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateWallpaperParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}
