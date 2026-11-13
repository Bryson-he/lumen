//! Shared message types for `lumen-core` <-> clients (the settings app, `lumen-ctl`).
//!
//! Transport: the named pipe [`PIPE_NAME`], carrying JSON-RPC 2.0, one JSON
//! object per line. Clients send requests; the core answers each one and also
//! pushes notifications ([`event`]) to every connected client.

use lumen_theme::{Color, Mode, Source, Style, Theme, Variant};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// Params: [`ApplyPresetParams`](super::ApplyPresetParams). Result: the applied `Theme`.
    pub const APPLY_PRESET: &str = "applyPreset";
}

/// Notification names pushed by the core.
pub mod event {
    /// Params: [`ThemeChanged`](super::ThemeChanged).
    pub const THEME_CHANGED: &str = "themeChanged";
    /// Params: [`GameModeChanged`](super::GameModeChanged).
    pub const GAME_MODE_CHANGED: &str = "gameModeChanged";
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
    pub kind: WallpaperKind,
    /// Colours extracted at import, best theme seed first.
    pub palette: Vec<Color>,
    /// Seconds since 1970 (UTC).
    pub added_at: u64,
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

/// Params of `applyPreset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyPresetParams {
    /// The preset's file name in `presets/`, without `.json`.
    pub id: String,
}
