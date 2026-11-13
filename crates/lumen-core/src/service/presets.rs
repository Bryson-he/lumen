//! The preset methods: save the look on the desktop, apply one, and manage
//! the list. A preset names monitors by number (`#2`) so that it means the
//! same thing on another PC; the conversions to and from this PC's monitor
//! ids are here.

use std::path::{Path, PathBuf};

use lumen_ipc::{
    ExportPresetParams, ImportPresetParams, Layout, MonitorInfo, Preset, PresetIdParams, PresetInfo, PresetWallpaper, RenamePresetParams, SavePresetParams,
    presets::{PRESET_EXTENSION, PRESET_FORMAT, monitor_number, monitor_ref},
};

use super::*;
use crate::store::now_secs;

const MAX_NAME_CHARS: usize = 80;

impl<D: Desktop> Service<D> {
    /// Handles a preset method, or returns `None` if `method` is not one.
    pub(super) fn handle_preset(&mut self, method: &str, params: Value) -> Option<Result<Value, RpcError>> {
        Some(match method {
            method::LIST_PRESETS => json(self.preset_list()),
            method::SAVE_PRESET => parse::<SavePresetParams>(params).and_then(|p| self.save_preset(&p.name)).and_then(json),
            method::APPLY_PRESET => parse::<PresetIdParams>(params).and_then(|p| self.apply_preset(&p.id)).and_then(json),
            method::RENAME_PRESET => parse::<RenamePresetParams>(params).and_then(|p| self.rename_preset(&p.id, &p.name)).and_then(json),
            method::DUPLICATE_PRESET => parse::<PresetIdParams>(params).and_then(|p| self.duplicate_preset(&p.id)).and_then(json),
            method::DELETE_PRESET => parse::<PresetIdParams>(params).and_then(|p| {
                self.store.delete_preset(&p.id).map_err(store_error)?;
                self.presets_changed();
                json(())
            }),
            method::EXPORT_PRESET => parse::<ExportPresetParams>(params).and_then(|p| {
                let path = lumen_file_path(&p.path)?;
                self.store.export_preset(&p.id, &path).map_err(store_error)?;
                info!("exported preset \"{}\" to {}", p.id, path.display());
                json(())
            }),
            method::IMPORT_PRESET => parse::<ImportPresetParams>(params).and_then(|p| {
                let (id, preset) = self.store.import_preset(Path::new(&p.path)).map_err(store_error)?;
                info!("imported preset \"{}\"", preset.name);
                self.presets_changed();
                json(self.store.preset_info(&id, &preset, false))
            }),
            _ => return None,
        })
    }

    pub(super) fn preset_list(&self) -> Vec<PresetInfo> {
        let active = self.store.active_preset();
        self.store.presets().iter().map(|(id, preset)| self.store.preset_info(id, preset, active.as_deref() == Some(id.as_str()))).collect()
    }

    /// Tells clients and the tray menu that the list of presets changed.
    pub(super) fn presets_changed(&self) {
        let list = self.preset_list();
        if let Some(mirror) = &self.preset_mirror {
            *mirror.lock().unwrap_or_else(|p| p.into_inner()) = list.clone();
        }
        self.emit(event::PRESETS_CHANGED, list);
    }

    /// The current look no longer matches the preset it came from.
    pub(super) fn look_changed_by_hand(&self) {
        if self.store.active_preset().is_some() && self.store.set_active_preset(None).is_ok() {
            self.presets_changed();
        }
    }

    /// Saves the look that is on the desktop now: the theme, each monitor's
    /// wallpaper and the widget layout.
    fn save_preset(&mut self, name: &str) -> Result<PresetInfo, RpcError> {
        let name = clean_name(name)?;
        // Saving while trying a theme out keeps that theme: it is the look on screen.
        if self.preview.is_some() {
            self.commit_preview()?;
        }
        let theme = self.committed.clone().ok_or_else(|| {
            RpcError::new(code::INVALID_PARAMS, "There is no look to save yet. Make a theme from a wallpaper or a colour first, then save it as a preset.")
        })?;

        let monitors = self.desktop.monitors().map_err(desktop_error)?;
        let mut wallpapers = Vec::new();
        for monitor in &monitors {
            let Some(path) = &monitor.wallpaper else { continue };
            // A wallpaper from outside the library is copied in, so the preset
            // still works if the original file is moved or deleted.
            let entry = self.store.wallpapers().into_iter().find(|w| same_file(&w.path, path)).map(Ok).unwrap_or_else(|| self.store.import_wallpaper(Path::new(path)));
            match entry {
                Ok(entry) => wallpapers.push(PresetWallpaper { monitor: monitor.number, file: entry.file }),
                Err(error) => warn!("monitor {}'s wallpaper is not in the preset: {error}", monitor.number),
            }
        }

        let now = now_secs();
        let preset = Preset {
            format_version: PRESET_FORMAT,
            name,
            created_at: now,
            last_used_at: now,
            theme,
            wallpapers,
            widgets: self.widgets.as_ref().map(|state| portable_layout(&state.layout, &monitors)),
            bar: Value::Null,
        };
        let id = self.store.new_preset_id(&preset.name);
        self.store.save_preset(&id, &preset).map_err(store_error)?;
        self.store.set_active_preset(Some(&id)).map_err(store_error)?;
        info!("saved preset \"{}\"", preset.name);
        self.presets_changed();
        Ok(self.store.preset_info(&id, &preset, true))
    }

    /// Applies a preset at once: wallpapers, theme and widget layout.
    fn apply_preset(&mut self, id: &str) -> Result<Theme, RpcError> {
        let mut preset = self.store.preset(id).map_err(store_error)?;
        let monitors = self.desktop.monitors().map_err(desktop_error)?;

        // Wallpapers for the monitors this PC has; images that have gone missing are skipped.
        let mut wallpapers: Vec<(Option<String>, PathBuf)> = preset
            .wallpapers
            .iter()
            .filter_map(|w| {
                let monitor = monitors.iter().find(|m| m.number == w.monitor)?;
                let image = self.store.resolve(&w.file);
                image.is_file().then(|| (Some(monitor.id.clone()), image))
            })
            .collect();
        // A preset with no wallpapers of its own falls back to its theme's source image, on every monitor.
        if preset.wallpapers.is_empty()
            && let Source::Wallpaper { path } = &preset.theme.source
        {
            let image = self.store.resolve(path);
            if image.is_file() {
                wallpapers.push((None, image));
            }
        }

        // A preset replaces whatever is being tried out.
        self.revert_preview(ThemeChangeReason::Revert)?;
        let plan = self.plan(&preset.theme, Some(wallpapers));
        self.desktop.apply(&plan).map_err(desktop_error)?;
        self.store.save_active_theme(&preset.theme).map_err(store_error)?;
        self.committed = Some(preset.theme.clone());
        if let (Some(layout), Some(widgets)) = (&preset.widgets, &self.widget_host) {
            widgets(WidgetCommand::SetLayout(Box::new(local_layout(layout, &monitors))));
        }

        preset.last_used_at = now_secs();
        self.store.save_preset(id, &preset).map_err(store_error)?;
        self.store.set_active_preset(Some(id)).map_err(store_error)?;
        info!("applied preset \"{}\"", preset.name);
        self.emit_theme(None, ThemeChangeReason::Preset);
        self.presets_changed();
        Ok(preset.theme)
    }

    fn rename_preset(&mut self, id: &str, name: &str) -> Result<PresetInfo, RpcError> {
        let mut preset = self.store.preset(id).map_err(store_error)?;
        preset.name = clean_name(name)?;
        self.store.save_preset(id, &preset).map_err(store_error)?;
        self.presets_changed();
        Ok(self.store.preset_info(id, &preset, self.store.active_preset().as_deref() == Some(id)))
    }

    fn duplicate_preset(&mut self, id: &str) -> Result<PresetInfo, RpcError> {
        let mut preset = self.store.preset(id).map_err(store_error)?;
        preset.name = clean_name(&format!("{} copy", preset.name))?;
        preset.created_at = now_secs();
        preset.last_used_at = 0;
        let new_id = self.store.new_preset_id(&preset.name);
        self.store.save_preset(&new_id, &preset).map_err(store_error)?;
        self.presets_changed();
        Ok(self.store.preset_info(&new_id, &preset, false))
    }
}

fn clean_name(name: &str) -> Result<String, RpcError> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() { Err(RpcError::new(code::INVALID_PARAMS, "a preset needs a name")) } else { Ok(name) }
}

/// Where to write an exported preset: a full path, ending in `.lumen`.
fn lumen_file_path(path: &str) -> Result<PathBuf, RpcError> {
    let mut path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(RpcError::new(code::INVALID_PARAMS, "the export path must be a full path"));
    }
    if path.extension().and_then(|e| e.to_str()).is_none_or(|e| !e.eq_ignore_ascii_case(PRESET_EXTENSION)) {
        let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(format!(".{PRESET_EXTENSION}"));
        path.set_file_name(name);
    }
    Ok(path)
}

fn same_file(a: &str, b: &str) -> bool {
    let tidy = |p: &str| p.replace('/', "\\").to_lowercase();
    tidy(a) == tidy(b)
}

/// A layout with this PC's monitor ids replaced by monitor numbers (`#2`).
/// Widgets on the main monitor carry no monitor at all, as before.
fn portable_layout(layout: &Layout, monitors: &[MonitorInfo]) -> Layout {
    let mut layout = layout.clone();
    for widget in &mut layout.widgets {
        widget.monitor = widget.monitor.as_ref().and_then(|id| monitors.iter().find(|m| &m.id == id)).filter(|m| !m.primary).map(|m| monitor_ref(m.number));
    }
    layout
}

/// The reverse: monitor numbers become this PC's ids. A widget for a monitor
/// this PC does not have lands on the main monitor.
fn local_layout(layout: &Layout, monitors: &[MonitorInfo]) -> Layout {
    let mut layout = layout.clone();
    for widget in &mut layout.widgets {
        widget.monitor = widget
            .monitor
            .as_deref()
            .and_then(monitor_number)
            .and_then(|number| monitors.iter().find(|m| m.number == number))
            .filter(|m| !m.primary)
            .map(|m| m.id.clone());
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(number: usize, id: &str, primary: bool) -> MonitorInfo {
        MonitorInfo { number, id: id.into(), x: 0, y: 0, width: 1920, height: 1080, primary, wallpaper: None }
    }

    #[test]
    fn layouts_travel_by_monitor_number() {
        let home = [monitor(1, "HOME-LEFT", true), monitor(2, "HOME-RIGHT", false)];
        let mut layout = Layout::default();
        layout.widgets[1].monitor = Some("HOME-RIGHT".into());
        layout.widgets[2].monitor = Some("HOME-LEFT".into());

        let portable = portable_layout(&layout, &home);
        assert_eq!(portable.widgets.iter().map(|w| w.monitor.as_deref()).collect::<Vec<_>>(), [None, Some("#2"), None], "the main monitor needs no name");

        // On a PC with two monitors the second widget goes to its monitor 2...
        let office = [monitor(1, "OFFICE-A", true), monitor(2, "OFFICE-B", false)];
        assert_eq!(local_layout(&portable, &office).widgets[1].monitor.as_deref(), Some("OFFICE-B"));
        // ...and on a laptop with one screen it lands on the main monitor.
        let laptop = [monitor(1, "LAPTOP", true)];
        assert_eq!(local_layout(&portable, &laptop).widgets[1].monitor, None);
    }

    #[test]
    fn export_paths_get_the_lumen_extension() {
        assert_eq!(lumen_file_path(r"C:\Looks\night").unwrap(), PathBuf::from(r"C:\Looks\night.lumen"));
        assert_eq!(lumen_file_path(r"C:\Looks\night.LUMEN").unwrap(), PathBuf::from(r"C:\Looks\night.LUMEN"));
        assert_eq!(lumen_file_path(r"C:\Looks\v1.2").unwrap(), PathBuf::from(r"C:\Looks\v1.2.lumen"));
        assert!(lumen_file_path("night.lumen").is_err(), "a bare file name could land anywhere");
    }

    #[test]
    fn names_are_trimmed_and_must_not_be_empty() {
        assert_eq!(clean_name("  Night Sky ").unwrap(), "Night Sky");
        assert!(clean_name("   ").is_err());
        assert_eq!(clean_name(&"x".repeat(200)).unwrap().len(), MAX_NAME_CHARS);
    }
}
