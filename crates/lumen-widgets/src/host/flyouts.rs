//! The volume flyout's place in the host: answering the volume keys, hearing
//! about volume changes, and showing the flyout for a moment.
//!
//! While the flyout is switched on, Lumen registers the three volume keys as
//! its own hotkeys and changes the volume itself, so Windows does not show
//! its flyout as well. Nothing exists between appearances: the flyout's
//! window is created when it is shown and destroyed once it has faded.

use lumen_ipc::flyout::FlyoutConfig;
use lumen_win::audio::{Level, Volume};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};

use super::*;
use crate::flyout;

pub(super) const HOTKEY_VOLUME_UP: i32 = 3;
pub(super) const HOTKEY_VOLUME_DOWN: i32 = 4;
pub(super) const HOTKEY_VOLUME_MUTE: i32 = 5;
pub(super) const TIMER_FLYOUT: usize = 5;
pub(super) const TIMER_FLYOUT_FADE: usize = 6;
/// Posted to the host's window when anything changes the volume or mute.
pub(super) const VOLUME_CHANGED: u32 = WM_APP + 12;

/// How long the flyout stays after the last change, then how it fades.
const STAY_MS: u32 = 1600;
const FADE_TICK_MS: u32 = 20;
const FADE_STEPS: u8 = 7;

pub(super) struct Shown {
    surface: Surface,
    level: Level,
    /// Steps of the fade still to go; `None` while fully shown.
    fading: Option<u8>,
}

impl Host {
    /// Applies new flyout settings: saves them, takes or gives back the
    /// volume keys, and reports.
    pub(super) fn set_flyouts(&mut self, config: FlyoutConfig) {
        if config == self.flyouts {
            return;
        }
        self.flyouts = config;
        if let Err(error) = save_flyouts(&self.options.flyouts_path, &self.flyouts) {
            warn!("{error}");
        }
        self.hide_flyout();
        self.sync_volume_keys();
        self.sync_suspension();
        (self.on_state)(Report::Flyouts(self.flyouts.clone()));
    }

    /// Lumen answers the volume keys only while its flyout is on and no
    /// fullscreen app or game is in front; otherwise they are Windows' again.
    pub(super) fn sync_volume_keys(&mut self) {
        let wanted = self.flyouts.volume && !self.paused;
        if wanted == self.volume.is_some() {
            return;
        }
        let keys = [(HOTKEY_VOLUME_UP, VK_VOLUME_UP), (HOTKEY_VOLUME_DOWN, VK_VOLUME_DOWN), (HOTKEY_VOLUME_MUTE, VK_VOLUME_MUTE)];
        if wanted {
            let Some(volume) = Volume::new(self.window.0 as isize, VOLUME_CHANGED) else {
                warn!("no access to the volume; the Windows flyout stays");
                return;
            };
            self.volume = Some(volume);
            for (id, key) in keys {
                // No MOD_NOREPEAT: holding a key keeps changing the volume.
                if unsafe { RegisterHotKey(Some(self.window), id, HOT_KEY_MODIFIERS(0), u32::from(key.0)) }.is_err() {
                    warn!("a volume key is taken by another app; Windows will handle that one");
                }
            }
        } else {
            for (id, _) in keys {
                unsafe {
                    let _ = UnregisterHotKey(Some(self.window), id);
                }
            }
            self.volume = None;
            self.hide_flyout();
        }
    }

    pub(super) fn on_volume_key(&mut self, hotkey: i32) {
        let Some(volume) = self.volume.as_mut() else { return };
        let level = match hotkey {
            HOTKEY_VOLUME_UP => volume.step(true),
            HOTKEY_VOLUME_DOWN => volume.step(false),
            _ => volume.toggle_mute(),
        };
        if let Some(level) = level {
            self.show_flyout(level);
        }
    }

    /// Something changed the volume: a key (ours, just now), an app's slider,
    /// a knob on the speakers.
    pub(super) fn on_volume_changed(&mut self) {
        if let Some(level) = self.volume.as_mut().and_then(Volume::level) {
            self.show_flyout(level);
        }
    }

    /// Shows the flyout as it would look now ("Show it" in the settings app).
    pub(super) fn show_flyout_now(&mut self) {
        // Works even while the flyout is switched off, to try it first.
        let level = match self.volume.as_mut() {
            Some(volume) => volume.level(),
            None => Volume::new(0, 0).and_then(|mut volume| volume.level()),
        };
        if let Some(level) = level {
            self.show_flyout(level);
        }
    }

    fn show_flyout(&mut self, level: Level) {
        // Never over a game, and not while rendering is switched off.
        if self.paused {
            return;
        }
        let Some(gpu) = self.gpu.clone() else { return };
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_FLYOUT_FADE);
            SetTimer(Some(self.window), TIMER_FLYOUT, STAY_MS, None);
        }
        if let Some(shown) = &mut self.flyout {
            let unchanged = shown.level == level && shown.fading.is_none();
            shown.level = level;
            if shown.fading.take().is_some() {
                shown.surface.set_opacity(1.0);
            }
            if unchanged {
                return;
            }
        } else {
            let area = self.area_index(None);
            let Some((work, scale)) = self.areas.get(area).map(|a| (a.work_rect, a.scale)) else { return };
            let (width, height) = flyout::size(self.flyouts.style);
            let physical = |v: f32| (v * scale).round() as i32;
            let (x, y) = flyout::origin(self.flyouts.position, work, (physical(width), physical(height)), physical(flyout::MARGIN));
            match Surface::new(&gpu, SurfaceOptions { x, y, width, height, click_through: true, topmost: true }) {
                Ok(surface) => self.flyout = Some(Shown { surface, level, fading: None }),
                Err(error) => {
                    warn!("could not create the flyout: {error}");
                    return;
                }
            }
        }
        self.draw_flyout();
    }

    /// Redraws the flyout if it is on screen (the volume or the theme changed).
    pub(super) fn draw_flyout(&mut self) {
        let Some(shown) = &self.flyout else { return };
        let (width, height) = shown.surface.size();
        let (paint, style, level, media) = (&self.paint, self.flyouts.style, shown.level, self.media.as_ref());
        if let Err(error) = shown.surface.draw(|canvas| flyout::draw(canvas, paint, style, level, media, width, height)) {
            warn!("{error}");
        }
    }

    /// Handles the flyout's two timers; `false` if the timer is not one of them.
    pub(super) fn on_flyout_timer(&mut self, timer: usize) -> bool {
        match timer {
            TIMER_FLYOUT => unsafe {
                let _ = KillTimer(Some(self.window), TIMER_FLYOUT);
                if let Some(shown) = &mut self.flyout {
                    shown.fading = Some(FADE_STEPS);
                    SetTimer(Some(self.window), TIMER_FLYOUT_FADE, FADE_TICK_MS, None);
                }
            },
            TIMER_FLYOUT_FADE => {
                let left = self.flyout.as_mut().and_then(|shown| {
                    let left = shown.fading?.saturating_sub(1);
                    shown.fading = Some(left);
                    shown.surface.set_opacity(f32::from(left) / f32::from(FADE_STEPS));
                    Some(left)
                });
                if left.is_none_or(|left| left == 0) {
                    self.hide_flyout();
                }
            }
            _ => return false,
        }
        true
    }

    /// Removes the flyout's window at once and stops its timers.
    pub(super) fn hide_flyout(&mut self) {
        self.flyout = None;
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_FLYOUT);
            let _ = KillTimer(Some(self.window), TIMER_FLYOUT_FADE);
        }
    }
}

/// Reads `flyouts.json`; a missing or broken file means the defaults (off).
pub(super) fn load_flyouts(path: &std::path::Path) -> FlyoutConfig {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_else(|error| {
            warn!("{} is not a valid flyout configuration ({error}); flyouts stay off", path.display());
            FlyoutConfig::default()
        }),
        Err(_) => FlyoutConfig::default(),
    }
}

fn save_flyouts(path: &std::path::Path, config: &FlyoutConfig) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("could not save {}: {error}", path.display());
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path)).map_err(failed)
}
