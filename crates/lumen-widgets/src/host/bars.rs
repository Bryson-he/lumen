//! The status bar's place in the host: where each bar sits, the strip of
//! screen reserved for it, its drawing surface, and what clicks on it do.
//!
//! A bar has two lives. Its *place* (a hidden anchor window registered with
//! Windows as an app bar, and the rectangle reserved for it) lasts as long as
//! the bar is switched on. Its *surface* (the window that is drawn) is
//! destroyed whenever rendering is suspended and made again afterwards. That
//! way a game starting or ending never makes other windows jump.

use lumen_ipc::{bar::BarPosition, presets::monitor_number};
use lumen_render::{Surface, SurfaceOptions};
use lumen_win::{
    appbar::{AppBar, Edge},
    shell,
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, RegisterClassW, WINDOW_STYLE, WNDCLASSW, WS_EX_TOOLWINDOW},
    },
    core::w,
};

use super::*;
use crate::bar::{self, Action, Region};

/// Launcher icons are read at this size and scaled down when drawn.
const ICON_PIXELS: u32 = 64;
/// The governor's name for the bar.
pub(super) const BAR_COST_ID: &str = "bar";

pub(super) struct BarSlot {
    /// Index into `Host::areas`.
    area: usize,
    /// Hidden window that owns the reservation and hears about changes to it.
    anchor: HWND,
    reservation: Option<AppBar>,
    /// Where the bar sits, in physical pixels: x, y, width, height.
    rect: (i32, i32, i32, i32),
    surface: Option<Surface>,
    regions: Vec<Region>,
}

impl Drop for BarSlot {
    fn drop(&mut self) {
        // Give the space back before the window that asked for it goes away.
        self.reservation = None;
        unsafe {
            let _ = DestroyWindow(self.anchor);
        }
    }
}

impl Host {
    /// Applies a new bar configuration: saves it, re-places the bars, and reports.
    pub(super) fn set_bar(&mut self, config: BarConfig) {
        let config = config.sanitised();
        if config == self.bar {
            return;
        }
        self.bar = config;
        if let Err(error) = save_bar(&self.options.bar_path, &self.bar) {
            warn!("{error}");
        }
        self.load_icons();
        self.place_bars();
        self.sync_suspension();
        self.sync_visibility();
        self.draw(None);
        self.sync_timers();
        (self.on_state)(Report::Bar(self.bar.clone()));
    }

    /// Reads the icons of the launcher's programs (once per program).
    pub(super) fn load_icons(&mut self) {
        let wanted: Vec<String> = bar::launcher_paths(&self.bar).into_iter().map(str::to_owned).collect();
        self.icon_pixels.retain(|path, _| wanted.contains(path));
        self.icons.retain(|path, _| wanted.contains(path));
        for path in wanted {
            self.icon_pixels.entry(path.clone()).or_insert_with(|| shell::file_icon(&path, ICON_PIXELS));
        }
    }

    /// The monitors the bar is shown on, as indexes into `areas`.
    fn bar_areas(&self) -> Vec<usize> {
        match self.bar.monitor.as_str() {
            "all" => (0..self.areas.len()).collect(),
            other => {
                let wanted = monitor_number(other).and_then(|number| self.areas.iter().position(|a| a.info.number == number));
                // A monitor that is not connected falls back to the main one.
                vec![wanted.unwrap_or_else(|| self.area_index(None))]
            }
        }
    }

    /// (Re)creates each bar's place: anchor, reserved strip, rectangle. Called
    /// when the bar's settings or the monitors change, not on every redraw.
    pub(super) fn place_bars(&mut self) {
        self.bars.clear();
        if self.bar.enabled {
            for area in self.bar_areas() {
                if let Some(slot) = self.place_bar(area) {
                    self.bars.push(slot);
                }
            }
        }
        // Reserving (or releasing) space changes each monitor's work area,
        // which widget positions are measured from.
        self.refresh_work_areas();
        self.make_bar_surfaces();
    }

    fn place_bar(&self, area_index: usize) -> Option<BarSlot> {
        let area = self.areas.get(area_index)?;
        let anchor = create_anchor()?;
        let thickness = (f32::from(self.bar.height) * area.scale).round() as i32;
        let edge = if self.bar.position == BarPosition::Top { Edge::Top } else { Edge::Bottom };

        let reservation = if self.bar.reserve_space { AppBar::register(anchor.0 as isize, BAR_CALLBACK) } else { None };
        let rect = match &reservation {
            Some(reservation) => reservation.reserve(edge, area.bounds, thickness),
            // Floating: along the edge of the work area, over whatever is there.
            None => {
                let (x, y, width, height) = area.work_rect;
                (x, if edge == Edge::Top { y } else { y + height - thickness }, width, thickness)
            }
        };
        Some(BarSlot { area: area_index, anchor, reservation, rect, surface: None, regions: Vec::new() })
    }

    /// Asks Windows again where each reserved bar may sit (something that
    /// holds screen space moved), and follows.
    pub(super) fn reposition_bars(&mut self) {
        let edge = if self.bar.position == BarPosition::Top { Edge::Top } else { Edge::Bottom };
        let mut moved = false;
        for index in 0..self.bars.len() {
            let Some(area) = self.areas.get(self.bars[index].area) else {
                continue;
            };
            let thickness = (f32::from(self.bar.height) * area.scale).round() as i32;
            let bounds = area.bounds;
            let slot = &mut self.bars[index];
            let Some(reservation) = &slot.reservation else {
                continue;
            };
            let rect = reservation.reserve(edge, bounds, thickness);
            if rect != slot.rect {
                slot.rect = rect;
                if let Some(surface) = slot.surface.as_mut() {
                    surface.set_position(rect.0, rect.1);
                }
                moved = true;
            }
        }
        if moved {
            self.refresh_work_areas();
        }
    }

    /// Re-reads the monitors' work areas and moves the widgets to match.
    fn refresh_work_areas(&mut self) {
        let fresh = read_areas();
        // Same monitors in the same order: only the work areas can have changed.
        if fresh.len() == self.areas.len() && fresh.iter().zip(&self.areas).all(|(a, b)| a.info.id == b.info.id) {
            self.areas = fresh;
            for index in 0..self.layout.widgets.len() {
                let placement = self.layout.widgets[index].clone();
                self.apply_placement(&placement, false);
            }
            self.sync_edit_surfaces();
        }
    }

    /// Creates the drawing surface for each placed bar, unless rendering is suspended.
    pub(super) fn make_bar_surfaces(&mut self) {
        let Some(gpu) = self.gpu.clone() else { return };
        for index in 0..self.bars.len() {
            let Some(scale) = self.areas.get(self.bars[index].area).map(|a| a.scale) else {
                continue;
            };
            let (x, y, width, height) = self.bars[index].rect;
            let options = SurfaceOptions { x, y, width: width as f32 / scale, height: height as f32 / scale, click_through: false, topmost: true };
            self.bars[index].surface = match Surface::new(&gpu, options) {
                Ok(surface) => Some(surface),
                Err(error) => {
                    warn!("could not create the bar: {error}");
                    None
                }
            };
        }
    }

    /// Destroys the bars' surfaces but keeps their places.
    pub(super) fn drop_bar_surfaces(&mut self) {
        for slot in &mut self.bars {
            slot.surface = None;
            slot.regions.clear();
        }
        self.icons.clear();
    }

    pub(super) fn any_bar_showing(&self) -> bool {
        self.bars.iter().any(|slot| slot.surface.is_some())
    }

    pub(super) fn draw_bars(&mut self) {
        if !self.any_bar_showing() {
            return;
        }
        let started = Instant::now();
        let data = bar::Data { clock: &self.clock, stats: &self.stats, media: self.media.as_ref(), weather: &self.weather.state };
        let (paint, config, icons, pixels) = (if self.bar.reserve_space { &self.paints.bar } else { &self.paints.bar_floating }, &self.bar, &mut self.icons, &self.icon_pixels);
        for slot in &mut self.bars {
            let Some(surface) = &slot.surface else {
                continue;
            };
            let (width, height) = surface.size();
            let mut regions = Vec::new();
            let result = surface.draw(|canvas| {
                // Upload any launcher icon not on the GPU yet.
                for (path, icon) in pixels {
                    if let Some(icon) = icon
                        && !icons.contains_key(path)
                        && let Ok(bitmap) = canvas.bitmap_from_rgba(icon.size, icon.size, &icon.rgba)
                    {
                        icons.insert(path.clone(), bitmap);
                    }
                }
                regions = bar::draw(canvas, paint, config, &data, icons, width, height);
            });
            if let Err(error) = result {
                warn!("{error}");
            }
            slot.regions = regions;
        }
        self.governor.record_draw(BAR_COST_ID, started.elapsed());
    }

    /// The memory the bars hold: their frame buffers and launcher icons.
    pub(super) fn bar_memory_bytes(&self) -> u64 {
        let surfaces: u64 = self.bars.iter().filter_map(|slot| slot.surface.as_ref()).map(Surface::memory_bytes).sum();
        let icons: u64 = self.icon_pixels.values().flatten().map(|icon| icon.rgba.len() as u64 * 2).sum();
        surfaces + icons
    }

    /// The action under the mouse on a bar window; `x` is physical pixels from its left end.
    fn bar_action(&self, window: isize, x: i32) -> Option<Action> {
        let slot = self.bars.iter().find(|slot| slot.surface.as_ref().is_some_and(|s| s.window() == window))?;
        let scale = slot.surface.as_ref()?.scale();
        bar::action_at(&slot.regions, x as f32 / scale).cloned()
    }

    pub(super) fn is_bar_window(&self, window: isize) -> bool {
        self.bars.iter().any(|slot| slot.surface.as_ref().is_some_and(|s| s.window() == window))
    }

    pub(super) fn on_bar_click(&mut self, window: isize, x: i32) {
        match self.bar_action(window, x) {
            Some(Action::OpenSettings) => {
                // The settings app sits next to the core.
                if let Ok(exe) = std::env::current_exe() {
                    let app = exe.with_file_name("lumen-settings.exe");
                    if !app.is_file() || !shell::open(&app.to_string_lossy()) {
                        info!("the settings app is not installed next to lumen-core");
                    }
                }
            }
            Some(Action::ToggleMedia) => media::toggle_play_pause(),
            Some(Action::Launch(path)) if !shell::open(&path) => warn!("could not open {path}"),
            _ => {}
        }
    }

    /// The screen x of a bar window's left end, in physical pixels.
    pub(super) fn bars_left_edge(&self, window: isize) -> Option<i32> {
        self.bars.iter().find(|slot| slot.surface.as_ref().is_some_and(|s| s.window() == window)).map(|slot| slot.rect.0)
    }

    /// Whether the mouse is over something clickable on a bar.
    pub(super) fn bar_hot(&self, window: isize, x: i32) -> bool {
        self.bar_action(window, x).is_some()
    }
}

/// Reads `bar.json`; a missing or broken file means the default (bar off).
pub(super) fn load_bar(path: &std::path::Path) -> BarConfig {
    match std::fs::read_to_string(path) {
        // Notepad and PowerShell may save the file with a byte-order mark.
        Ok(text) => serde_json::from_str::<BarConfig>(text.trim_start_matches('\u{feff}')).map(BarConfig::sanitised).unwrap_or_else(|error| {
            warn!("{} is not a valid bar configuration ({error}); the bar stays off", path.display());
            BarConfig::default()
        }),
        Err(_) => BarConfig::default(),
    }
}

fn save_bar(path: &std::path::Path, config: &BarConfig) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("could not save {}: {error}", path.display());
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path)).map_err(failed)
}

/// A hidden window to own one bar's reservation. Its messages go to the
/// host's window procedure.
fn create_anchor() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = w!("LumenBarAnchor");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
        CreateWindowExW(WS_EX_TOOLWINDOW, class, w!("Lumen bar"), WINDOW_STYLE(0), 0, 0, 0, 0, None, None, Some(instance.into()), None).ok()
    }
}
