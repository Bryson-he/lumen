//! Component styles in the host: the user's style sheet, and the paint each
//! kind of piece draws with (the theme's paint with that piece's style applied).

use lumen_ipc::style::StyleSheet;

use super::*;

/// One resolved paint per kind of piece.
pub(super) struct Paints {
    pub(super) widgets: Paint,
    /// A bar with its own strip of the screen: only wallpaper is behind it.
    pub(super) bar: Paint,
    /// A bar floating over windows.
    pub(super) bar_floating: Paint,
    pub(super) flyout: Paint,
    pub(super) launcher: Paint,
}

impl Paints {
    /// Every piece in the theme's own look.
    pub(super) fn plain(paint: &Paint) -> Self {
        Self::resolve(paint, &StyleSheet::default())
    }

    fn resolve(paint: &Paint, sheet: &StyleSheet) -> Self {
        let assigned = &sheet.assigned;
        let bar = sheet.resolve(&assigned.bar);
        Self {
            widgets: paint.styled(&sheet.resolve(&assigned.widgets), false),
            bar: paint.styled(&bar, false),
            bar_floating: paint.styled(&bar, true),
            flyout: paint.styled(&sheet.resolve(&assigned.flyout), true),
            launcher: paint.styled(&sheet.resolve(&assigned.launcher), true),
        }
    }
}

impl Host {
    /// Works the paints out again (the theme or the styles changed).
    pub(super) fn refresh_paints(&mut self) {
        self.paints = Paints::resolve(&self.paint, &self.styles);
    }

    /// What one widget draws with: its own component style if it has one,
    /// else the one all widgets share; then its own background and corners.
    pub(super) fn widget_paint(&self, placement: &Placement) -> Paint {
        match placement.component_style.as_deref().filter(|id| self.styles.exists(id)) {
            Some(id) => self.paint.styled(&self.styles.resolve(id), false).for_widget(&placement.style),
            None => self.paints.widgets.for_widget(&placement.style),
        }
    }

    /// Applies a new style sheet: saves it, redraws everything, and reports.
    pub(super) fn set_styles(&mut self, sheet: StyleSheet) {
        let sheet = sheet.sanitised();
        if sheet == self.styles {
            return;
        }
        self.styles = sheet;
        if let Err(error) = save_styles(&self.options.styles_path, &self.styles) {
            warn!("{error}");
        }
        // A widget whose own style was deleted goes back to the shared one.
        let mut layout_changed = false;
        for widget in &mut self.layout.widgets {
            if widget.component_style.as_deref().is_some_and(|id| !self.styles.exists(id)) {
                widget.component_style = None;
                layout_changed = true;
            }
        }
        self.refresh_paints();
        self.draw(None);
        self.draw_flyout();
        self.draw_launcher();
        self.sync_edit_surfaces();
        if layout_changed {
            self.save();
            self.report();
        }
        (self.on_state)(Report::Styles(self.styles.clone()));
    }
}

/// Reads `styles.json`; a missing or broken file means no styles of the user's own.
pub(super) fn load_styles(path: &std::path::Path) -> StyleSheet {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<StyleSheet>(text.trim_start_matches('\u{feff}')).map(StyleSheet::sanitised).unwrap_or_else(|error| {
            warn!("{} is not a valid style file ({error}); using the built-in styles", path.display());
            StyleSheet::default()
        }),
        Err(_) => StyleSheet::default(),
    }
}

fn save_styles(path: &std::path::Path, sheet: &StyleSheet) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("could not save {}: {error}", path.display());
    let text = serde_json::to_string_pretty(sheet).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path)).map_err(failed)
}
