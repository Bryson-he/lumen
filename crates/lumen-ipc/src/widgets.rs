//! Desktop widget layout: the shape of `widgets/layout.json` and of the
//! widget messages between the core and the settings app.

use serde::{Deserialize, Serialize};

/// Widgets snap to multiples of this many logical pixels.
pub const GRID: f32 = 24.0;
/// The smallest a widget can be made, in logical pixels.
pub const MIN_SIZE: (f32, f32) = (96.0, 56.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidgetKind {
    Clock,
    SystemStats,
    NowPlaying,
}

impl WidgetKind {
    pub const ALL: [Self; 3] = [Self::Clock, Self::SystemStats, Self::NowPlaying];

    /// Default size in logical pixels.
    pub const fn default_size(self) -> (f32, f32) {
        match self {
            Self::Clock => (190.0, 92.0),
            Self::SystemStats => (190.0, 150.0),
            Self::NowPlaying => (320.0, 96.0),
        }
    }

    /// The start of ids for this kind: `clock-1`, `system-2`, ...
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::SystemStats => "system",
            Self::NowPlaying => "now-playing",
        }
    }
}

/// What is drawn behind a widget's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Background {
    /// See-through, following the theme's blur setting.
    #[default]
    Glass,
    /// The theme's surface colour, fully opaque.
    Solid,
    None,
}

/// Per-widget look and behaviour (the inspector's Style and Data tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetStyle {
    pub background: Background,
    /// Whole-widget opacity, 20–100 percent.
    pub opacity: u8,
    /// Corner radius in pixels; `None` follows the theme.
    pub radius: Option<u8>,
    /// Invisible until the mouse is over it.
    pub show_on_hover: bool,
    /// Hidden (and not drawn) while a fullscreen app or game is in front.
    pub hide_in_games: bool,
}

impl Default for WidgetStyle {
    fn default() -> Self {
        Self { background: Background::Glass, opacity: 100, radius: None, show_on_hover: false, hide_in_games: true }
    }
}

/// One widget on the desktop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub kind: WidgetKind,
    /// The monitor it is on (Windows' monitor id); the primary monitor if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Top-left corner, in logical pixels from the top-left of that monitor's work area.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub style: WidgetStyle,
    /// A component style for this widget only; absent uses the style assigned to all widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_style: Option<String>,
}

/// `widgets/layout.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub widgets: Vec<Placement>,
    /// Snap to the grid and to other widgets while moving.
    #[serde(default = "yes")]
    pub snap: bool,
    /// Widgets cannot be moved or resized.
    #[serde(default)]
    pub locked: bool,
}

fn yes() -> bool {
    true
}

impl Default for Layout {
    /// The first-run layout: clock, system stats and now playing down the left edge.
    fn default() -> Self {
        let mut y = 48.0;
        let widgets = WidgetKind::ALL
            .into_iter()
            .map(|kind| {
                let (width, height) = kind.default_size();
                let placement = Placement { id: format!("{}-1", kind.id_prefix()), kind, monitor: None, x: 48.0, y, width, height, style: WidgetStyle::default(), component_style: None };
                y += height + GRID;
                placement
            })
            .collect();
        Self { widgets, snap: true, locked: false }
    }
}

impl Layout {
    /// Keeps hand-edited files from producing absurd windows.
    pub fn sanitised(mut self) -> Self {
        self.widgets.retain(|w| w.x.is_finite() && w.y.is_finite() && w.width.is_finite() && w.height.is_finite() && !w.id.is_empty());
        for widget in &mut self.widgets {
            widget.width = widget.width.clamp(MIN_SIZE.0, 2000.0);
            widget.height = widget.height.clamp(MIN_SIZE.1, 2000.0);
            widget.style.opacity = widget.style.opacity.clamp(20, 100);
        }
        self
    }

    /// An id not used yet: `clock-2`, `clock-3`, ...
    pub fn next_id(&self, kind: WidgetKind) -> String {
        (1..).map(|n| format!("{}-{n}", kind.id_prefix())).find(|id| !self.widgets.iter().any(|w| &w.id == id)).expect("an unused number always exists")
    }
}

/// A monitor widgets can be placed on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorArea {
    pub id: String,
    /// 1-based, left to right then top to bottom.
    pub number: usize,
    pub primary: bool,
    /// Size of its work area (the monitor minus the taskbar) in logical pixels.
    pub width: f32,
    pub height: f32,
    /// The image it shows, for the layout preview.
    pub wallpaper: Option<String>,
}

/// Alignment lines to show while a widget is being dragged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guides {
    /// A vertical line at this x (logical pixels in the monitor's work area).
    pub x: Option<f32>,
    /// A horizontal line at this y.
    pub y: Option<f32>,
    pub monitor: Option<String>,
}

/// Result of `getWidgets` and params of the `widgetsChanged` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetsState {
    pub layout: Layout,
    pub monitors: Vec<MonitorArea>,
    /// Desktop edit mode (Win+Alt+E) is on.
    pub edit_mode: bool,
    pub selected: Option<String>,
    pub guides: Guides,
    /// Widgets the governor has switched off for using too much CPU.
    #[serde(default)]
    pub suspended: Vec<String>,
}

/// What one widget may use before the governor steps in: its share of the
/// whole CPU, in percent, and its memory in megabytes.
pub const WIDGET_CPU_BUDGET: f64 = 0.1;
pub const WIDGET_MEMORY_BUDGET_MB: f64 = 5.0;

/// How a widget stands against its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostStatus {
    #[default]
    Ok,
    /// Over budget; shown as a warning.
    OverBudget,
    /// Far over budget for a sustained time; switched off until resumed.
    Suspended,
}

/// One widget's measured cost. Result items of `getWidgetCosts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetCost {
    pub id: String,
    /// Share of the whole CPU spent on this widget, in percent, over the last measuring window.
    pub cpu_percent: f64,
    /// Memory held for this widget (its drawing surfaces and images), in megabytes.
    pub memory_mb: f64,
    /// How often it redrew, per minute, over the last measuring window.
    pub draws_per_minute: f64,
    pub status: CostStatus,
}

/// Params of `updateWidget`. Only the fields given are changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateWidgetParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Move it to this monitor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<WidgetStyle>,
    /// Give this widget its own component style; an empty string goes back to the one all widgets share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_style: Option<String>,
    /// True while the user is still dragging: the position is snapped and
    /// shown with guides, but not written to disk until a final update.
    #[serde(default)]
    pub dragging: bool,
}

/// Params of `addWidget`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddWidgetParams {
    pub kind: WidgetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Where to put it; a free spot is found if left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
}

/// Params of `removeWidget`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WidgetIdParams {
    pub id: String,
}

/// Params of `selectWidget`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectWidgetParams {
    /// `null` clears the selection.
    #[serde(default)]
    pub id: Option<String>,
}

/// Params of `setLayoutOptions`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutOptionsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}

/// Params of `setEditMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditModeParams {
    pub on: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_the_three_widgets_without_overlap() {
        let layout = Layout::default();
        assert_eq!(layout.widgets.iter().map(|w| w.kind).collect::<Vec<_>>(), WidgetKind::ALL);
        assert!(layout.snap && !layout.locked);
        for pair in layout.widgets.windows(2) {
            assert!(pair[0].y + pair[0].height < pair[1].y, "{} overlaps {}", pair[0].id, pair[1].id);
        }
    }

    #[test]
    fn a_layout_written_before_styles_existed_still_loads() {
        let old = r#"{"widgets":[{"id":"clock-1","kind":"clock","x":48.0,"y":48.0,"width":190.0,"height":92.0}]}"#;
        let layout: Layout = serde_json::from_str(old).unwrap();
        assert_eq!(layout.widgets[0].style, WidgetStyle::default());
        assert!(layout.snap, "snapping is on unless turned off");
    }

    #[test]
    fn styles_and_options_round_trip() {
        let mut layout = Layout { locked: true, snap: false, ..Default::default() };
        layout.widgets[2].monitor = Some("MONITOR-B".into());
        layout.widgets[2].style = WidgetStyle { background: Background::Solid, opacity: 80, radius: Some(4), show_on_hover: true, hide_in_games: false };
        let text = serde_json::to_string(&layout).unwrap();
        assert!(text.contains(r#""background":"solid""#) && text.contains(r#""showOnHover":true"#) && text.contains(r#""kind":"nowPlaying""#), "{text}");
        assert_eq!(serde_json::from_str::<Layout>(&text).unwrap(), layout);
    }

    #[test]
    fn new_ids_do_not_collide() {
        let mut layout = Layout::default();
        assert_eq!(layout.next_id(WidgetKind::Clock), "clock-2");
        layout.widgets.retain(|w| w.kind != WidgetKind::Clock);
        assert_eq!(layout.next_id(WidgetKind::Clock), "clock-1");
    }

    #[test]
    fn wild_values_are_clamped_and_broken_entries_dropped() {
        let text = r#"{"widgets":[
            {"id":"a","kind":"clock","x":0,"y":0,"width":1,"height":99999,"style":{"opacity":3}},
            {"id":"","kind":"clock","x":0,"y":0,"width":100,"height":100}]}"#;
        let layout = serde_json::from_str::<Layout>(text).unwrap().sanitised();
        assert_eq!(layout.widgets.len(), 1);
        assert_eq!((layout.widgets[0].width, layout.widgets[0].height, layout.widgets[0].style.opacity), (MIN_SIZE.0, 2000.0, 20));
    }
}
