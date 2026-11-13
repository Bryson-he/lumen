//! The launcher's place in the host: its hotkey, the panel while it is open,
//! typing and picking, and opening the chosen app.
//!
//! The panel is the one Lumen window that takes the keyboard, and only
//! because its hotkey was just pressed. It exists only while open: closing it
//! destroys its window and forgets the icons it loaded.

use lumen_ipc::launcher::{LauncherConfig, LauncherHotkey};
use lumen_win::{
    apps::{self, App},
    shell,
};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{MOD_CONTROL, VK_DOWN, VK_RETURN, VK_SPACE, VK_UP},
    WindowsAndMessaging::{WM_ACTIVATE, WM_CHAR, WM_KEYDOWN},
};

use super::*;
use crate::launcher;

pub(super) const HOTKEY_LAUNCHER: i32 = 6;
/// Icons are read at this size and scaled down when drawn.
const ICON_PIXELS: u32 = 48;
/// The list of installed apps is read again if it is older than this.
const APPS_FRESH_FOR: Duration = Duration::from_secs(120);

pub(super) struct Open {
    surface: Surface,
    query: String,
    results: Vec<App>,
    selected: usize,
    /// Icons of the apps shown so far (`None`: it has none), and on the GPU.
    icon_pixels: HashMap<String, Option<shell::Icon>>,
    icons: HashMap<String, Bitmap>,
}

/// What the host remembers about the launcher between openings.
#[derive(Default)]
pub(super) struct Launcher {
    pub(super) config: LauncherConfig,
    /// The hotkey registered right now, if any.
    registered: Option<LauncherHotkey>,
    apps: Vec<App>,
    apps_read_at: Option<Instant>,
    /// How many times each app was opened from the launcher.
    usage: HashMap<String, u32>,
    pub(super) open: Option<Open>,
}

impl Host {
    pub(super) fn load_launcher(&mut self) {
        self.launcher.config = load_json(&self.options.launcher_path, "launcher configuration").unwrap_or_default();
        self.launcher.usage = load_json(&usage_path(&self.options.launcher_path), "launcher history").unwrap_or_default();
    }

    /// Applies new launcher settings: saves them, moves the hotkey, and reports.
    pub(super) fn set_launcher(&mut self, config: LauncherConfig) {
        if config == self.launcher.config {
            return;
        }
        self.launcher.config = config;
        if let Err(error) = save_json(&self.options.launcher_path, &self.launcher.config) {
            warn!("{error}");
        }
        self.close_launcher();
        self.sync_launcher_hotkey();
        self.sync_suspension();
        (self.on_state)(Report::Launcher(self.launcher.config.clone()));
    }

    /// The hotkey is Lumen's only while the launcher is on and no fullscreen
    /// app or game is in front.
    pub(super) fn sync_launcher_hotkey(&mut self) {
        let wanted = (self.launcher.config.enabled && !self.paused).then_some(self.launcher.config.hotkey);
        if wanted == self.launcher.registered {
            return;
        }
        unsafe {
            if self.launcher.registered.take().is_some() {
                let _ = UnregisterHotKey(Some(self.window), HOTKEY_LAUNCHER);
            }
            if let Some(hotkey) = wanted {
                let modifiers = match hotkey {
                    LauncherHotkey::AltSpace => MOD_ALT,
                    LauncherHotkey::CtrlSpace => MOD_CONTROL,
                    LauncherHotkey::WinAltSpace => MOD_WIN | MOD_ALT,
                };
                if RegisterHotKey(Some(self.window), HOTKEY_LAUNCHER, modifiers | MOD_NOREPEAT, u32::from(VK_SPACE.0)).is_ok() {
                    self.launcher.registered = Some(hotkey);
                } else {
                    warn!("the launcher's hotkey is taken by another app; pick another one in Lumen");
                }
            }
        }
        if wanted.is_none() {
            self.close_launcher();
        }
    }

    /// The hotkey was pressed: open the panel, or close it if it is open.
    pub(super) fn toggle_launcher(&mut self) {
        if self.launcher.open.is_some() {
            self.close_launcher();
        } else {
            self.open_launcher();
        }
    }

    fn open_launcher(&mut self) {
        // Never over a game, and not while widgets are being arranged.
        if self.paused || self.edit.is_some() {
            return;
        }
        let Some(gpu) = self.gpu.clone() else { return };
        if self.launcher.apps_read_at.is_none_or(|at| at.elapsed() > APPS_FRESH_FOR) {
            self.launcher.apps = apps::installed();
            self.launcher.apps_read_at = Some(Instant::now());
        }

        // On the monitor the mouse is on, a little above the middle.
        let mut cursor = POINT::default();
        let _ = unsafe { GetCursorPos(&mut cursor) };
        let area = self.area_at(cursor.x, cursor.y).unwrap_or_else(|| self.area_index(None));
        let Some(((work_x, work_y, work_width, work_height), scale)) = self.areas.get(area).map(|a| (a.work_rect, a.scale)) else { return };
        let width = (launcher::WIDTH * scale).round() as i32;
        let (x, y) = (work_x + (work_width - width) / 2, work_y + work_height / 4);

        let height = launcher::height(launcher::MAX_RESULTS);
        let mut surface = match Surface::new(&gpu, SurfaceOptions { x, y, width: launcher::WIDTH, height, click_through: false, topmost: true }) {
            Ok(surface) => surface,
            Err(error) => {
                warn!("could not create the launcher: {error}");
                return;
            }
        };
        if !surface.take_keyboard() {
            // Without the keyboard it could be neither typed into nor closed with Esc.
            info!("Windows did not give the launcher the keyboard");
            return;
        }
        self.launcher.open = Some(Open { surface, query: String::new(), results: Vec::new(), selected: 0, icon_pixels: HashMap::new(), icons: HashMap::new() });
        self.refresh_launcher();
    }

    pub(super) fn close_launcher(&mut self) {
        self.launcher.open = None;
    }

    /// Works out the results for what is typed, sizes the panel and draws it.
    fn refresh_launcher(&mut self) {
        let Some(open) = &mut self.launcher.open else { return };
        open.results = launcher::search(&self.launcher.apps, &open.query, &self.launcher.usage, launcher::MAX_RESULTS).into_iter().cloned().collect();
        open.selected = open.selected.min(open.results.len().saturating_sub(1));
        for app in &open.results {
            open.icon_pixels.entry(app.target.clone()).or_insert_with(|| shell::file_icon(&app.target, ICON_PIXELS));
        }
        if let Err(error) = open.surface.resize(launcher::WIDTH, launcher::height(open.results.len())) {
            warn!("{error}");
        }
        self.draw_launcher();
    }

    /// Redraws the panel if it is open (typing, moving the selection, a new theme).
    pub(super) fn draw_launcher(&mut self) {
        let Some(open) = &mut self.launcher.open else { return };
        let (width, height) = open.surface.size();
        let paint = &self.paints.launcher;
        let Open { surface, query, results, selected, icon_pixels, icons } = open;
        let shown: Vec<&App> = results.iter().collect();
        let result = surface.draw(|canvas| {
            for (target, icon) in icon_pixels.iter() {
                if let Some(icon) = icon
                    && !icons.contains_key(target)
                    && let Ok(bitmap) = canvas.bitmap_from_rgba(icon.size, icon.size, &icon.rgba)
                {
                    icons.insert(target.clone(), bitmap);
                }
            }
            launcher::draw(canvas, paint, &launcher::View { query, results: &shown, selected: *selected }, icons, width, height);
        });
        if let Err(error) = result {
            warn!("{error}");
        }
    }

    pub(super) fn is_launcher_window(&self, window: isize) -> bool {
        self.launcher.open.as_ref().is_some_and(|open| open.surface.window() == window)
    }

    /// Keyboard, activation and click messages for the open panel.
    pub(super) fn on_launcher_message(&mut self, message: u32, wparam: usize, y: i32) {
        let Some(open) = &mut self.launcher.open else { return };
        match message {
            WM_CHAR => {
                let Some(typed) = char::from_u32(wparam as u32) else { return };
                match typed {
                    '\u{8}' => {
                        open.query.pop();
                    }
                    // Ctrl+Backspace: clear the lot.
                    '\u{7f}' => open.query.clear(),
                    c if !c.is_control() && open.query.chars().count() < launcher::MAX_QUERY => open.query.push(c),
                    _ => return,
                }
                open.selected = 0;
                self.refresh_launcher();
            }
            WM_KEYDOWN => {
                let key = wparam as u16;
                if key == VK_ESCAPE.0 {
                    self.close_launcher();
                } else if key == VK_RETURN.0 {
                    self.launch_selected();
                } else if key == VK_DOWN.0 || key == VK_UP.0 {
                    let count = open.results.len();
                    if count > 0 {
                        // Wraps round at both ends.
                        open.selected = if key == VK_DOWN.0 { (open.selected + 1) % count } else { (open.selected + count - 1) % count };
                        self.draw_launcher();
                    }
                }
            }
            // Another window was clicked or switched to (the low word 0 means "now inactive").
            WM_ACTIVATE if wparam & 0xFFFF == 0 => self.close_launcher(),
            WM_LBUTTONUP => {
                let scale = open.surface.scale();
                if let Some(row) = launcher::row_at(y as f32 / scale, open.results.len()) {
                    open.selected = row;
                    self.launch_selected();
                }
            }
            _ => {}
        }
    }

    fn launch_selected(&mut self) {
        let Some(app) = self.launcher.open.as_ref().and_then(|open| open.results.get(open.selected).cloned()) else { return };
        // Close first: the app's window then takes over from nothing, not from the panel.
        self.close_launcher();
        if shell::open(&app.target) {
            *self.launcher.usage.entry(app.target).or_insert(0) += 1;
            // Only apps that still exist are worth remembering.
            let known: Vec<&String> = self.launcher.apps.iter().map(|app| &app.target).collect();
            self.launcher.usage.retain(|target, _| known.contains(&target));
            if let Err(error) = save_json(&usage_path(&self.options.launcher_path), &self.launcher.usage) {
                warn!("{error}");
            }
        } else {
            warn!("could not open {}", app.name);
        }
    }
}

/// `launcher-usage.json`, beside `launcher.json`.
fn usage_path(config: &std::path::Path) -> PathBuf {
    config.with_file_name("launcher-usage.json")
}

fn load_json<T: serde::de::DeserializeOwned>(path: &std::path::Path, what: &str) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(value) => Some(value),
        Err(error) => {
            warn!("{} is not a valid {what} ({error}); starting afresh", path.display());
            None
        }
    }
}

fn save_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("could not save {}: {error}", path.display());
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path)).map_err(failed)
}
