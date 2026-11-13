//! Widget parts: the pieces a widget is made of (a line of text, a level
//! bar, a picture), each with its own place inside the widget and its own
//! look. A widget's default arrangement lives here; a widget whose parts the
//! user has changed carries its own list in the layout.
//!
//! Like component styles, a part never holds a colour, only which theme
//! colour to use.

use serde::{Deserialize, Serialize};

use crate::{style::Role, widgets::WidgetKind};

/// The most parts one widget holds.
pub const MAX_PARTS: usize = 32;
/// Text size limits, in logical pixels.
pub const TEXT_SIZE_RANGE: (f32, f32) = (6.0, 200.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    #[default]
    Text,
    /// A level bar, filled as far as its value says.
    Bar,
    /// A picture (the cover of what is playing).
    Image,
    /// A filled box: a panel behind other parts, a dot, or (made thin) a divider.
    Shape,
}

/// Which colour a part uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartColor {
    /// The widget's text colour, as its component style says.
    #[default]
    Text,
    /// The text colour, a little fainter.
    Muted,
    /// The widget's highlight colour, as its component style says.
    Highlight,
    Primary,
    Secondary,
    Tertiary,
    Neutral,
    Surface,
    OnSurface,
    Outline,
    Accent,
}

impl PartColor {
    /// The theme role, for the choices that name one directly.
    pub fn role(self) -> Option<Role> {
        Some(match self {
            Self::Text | Self::Muted | Self::Highlight => return None,
            Self::Primary => Role::Primary,
            Self::Secondary => Role::Secondary,
            Self::Tertiary => Role::Tertiary,
            Self::Neutral => Role::Neutral,
            Self::Surface => Role::Surface,
            Self::OnSurface => Role::OnSurface,
            Self::Outline => Role::Outline,
            Self::Accent => Role::Accent,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartAlign {
    #[default]
    Left,
    Centre,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartWeight {
    #[default]
    Regular,
    Medium,
    SemiBold,
}

/// One piece of a widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Part {
    /// Unique within the widget: "time", "cpu-bar".
    pub id: String,
    /// What the user sees it called: "Time", "CPU bar".
    pub name: String,
    pub kind: PartKind,
    /// The live value shown: "clock.time", "stats.cpu", "media.title"…
    /// Empty for fixed text, which is in `text`.
    pub source: String,
    /// Text, used when `source` is empty. Live values can be mixed in with
    /// braces: `CPU {stats.cpu}`.
    pub text: String,
    /// Top-left corner inside the widget, in logical pixels, at the widget's default size.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Grow and shrink with the widget's width (keeps its distance to the right edge).
    pub stretch: bool,
    /// Stay the same distance from the widget's bottom edge instead of its top.
    pub from_bottom: bool,
    /// Font family; `None` uses the widget's font (its component style's, else the theme's).
    pub font: Option<String>,
    /// Text size in logical pixels (a bar's or picture's size comes from its box).
    pub size: f32,
    pub weight: PartWeight,
    pub color: PartColor,
    /// 10–100 percent.
    pub opacity: u8,
    pub align: PartAlign,
    pub hidden: bool,
    /// Corner radius of a shape or picture, in logical pixels.
    pub radius: f32,
}

impl Default for Part {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: PartKind::Text,
            source: String::new(),
            text: String::new(),
            x: 16.0,
            y: 16.0,
            width: 120.0,
            height: 18.0,
            stretch: false,
            from_bottom: false,
            font: None,
            size: 13.0,
            weight: PartWeight::Regular,
            color: PartColor::Text,
            opacity: 100,
            align: PartAlign::Left,
            hidden: false,
            radius: 0.0,
        }
    }
}

impl Part {
    fn sanitised(mut self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        self.x = finite(self.x, 0.0).clamp(-2000.0, 2000.0);
        self.y = finite(self.y, 0.0).clamp(-2000.0, 2000.0);
        self.width = finite(self.width, 120.0).clamp(1.0, 2000.0);
        self.height = finite(self.height, 18.0).clamp(1.0, 2000.0);
        self.size = finite(self.size, 13.0).clamp(TEXT_SIZE_RANGE.0, TEXT_SIZE_RANGE.1);
        self.radius = finite(self.radius, 0.0).clamp(0.0, 1000.0);
        self.opacity = self.opacity.clamp(10, 100);
        self.name = self.name.trim().chars().take(40).collect();
        self.text = self.text.chars().take(200).collect();
        self.source = self.source.trim().chars().take(64).collect();
        self.font = self.font.map(|font| font.trim().chars().take(64).collect::<String>()).filter(|font| !font.is_empty());
        if self.name.is_empty() {
            self.name = self.id.clone();
        }
        self
    }

    /// Where the part sits in a widget of this size, given the size its
    /// position was written for: x, y, width, height.
    pub fn rect_in(&self, widget: (f32, f32), default: (f32, f32)) -> (f32, f32, f32, f32) {
        let width = if self.stretch { (self.width + widget.0 - default.0).max(1.0) } else { self.width };
        let y = if self.from_bottom { self.y + widget.1 - default.1 } else { self.y };
        (self.x, y, width, self.height)
    }
}

/// Keeps a list of parts within sensible limits: ids present and unique, values in range.
pub fn sanitise_parts(parts: Vec<Part>) -> Vec<Part> {
    let mut seen: Vec<String> = Vec::new();
    parts
        .into_iter()
        .filter(|part| {
            let ok = !part.id.is_empty() && part.id.len() <= 48 && part.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !seen.contains(&part.id);
            if ok {
                seen.push(part.id.clone());
            }
            ok
        })
        .take(MAX_PARTS)
        .map(Part::sanitised)
        .collect()
}

/// The size a kind of widget starts at, which its default parts are laid out for.
pub fn default_size(kind: WidgetKind) -> (f32, f32) {
    kind.default_size()
}

/// What `listFonts` returns.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontList {
    /// Shipped with Lumen.
    pub bundled: Vec<String>,
    /// Font files the user added to Lumen.
    pub imported: Vec<String>,
    /// Installed in Windows.
    pub installed: Vec<String>,
}

/// Params of `importFont`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFontParams {
    pub path: String,
}

/// The arrangement a kind of widget comes with.
pub fn default_parts(kind: WidgetKind) -> Vec<Part> {
    let text = |id: &str, name: &str, source: &str, y: f32, width: f32, height: f32| Part {
        id: id.to_owned(),
        name: name.to_owned(),
        source: source.to_owned(),
        y,
        width,
        height,
        stretch: true,
        ..Default::default()
    };
    match kind {
        WidgetKind::Clock => vec![
            Part { color: PartColor::Muted, ..text("date", "Date", "clock.date", 13.0, 158.0, 18.0) },
            Part { font: Some("Space Grotesk".to_owned()), size: 40.0, weight: PartWeight::SemiBold, ..text("time", "Time", "clock.time", 29.0, 158.0, 52.0) },
            Part { weight: PartWeight::Medium, color: PartColor::Muted, align: PartAlign::Right, ..text("suffix", "AM / PM", "clock.suffix", 55.0, 158.0, 18.0) },
        ],
        WidgetKind::SystemStats => {
            let mut parts = vec![Part { text: "System".to_owned(), weight: PartWeight::SemiBold, ..text("title", "Title", "", 13.0, 158.0, 18.0) }];
            let rows = [("cpu", "CPU", "stats.cpu", PartColor::Highlight), ("ram", "RAM", "stats.memory", PartColor::Tertiary), ("gpu", "GPU", "stats.gpu", PartColor::Accent)];
            for (index, (id, label, source, colour)) in rows.into_iter().enumerate() {
                let y = 40.0 + index as f32 * 35.0;
                parts.push(Part { text: label.to_owned(), ..text(&format!("{id}-label"), &format!("{label} label"), "", y, 158.0, 18.0) });
                parts.push(Part { color: PartColor::Muted, align: PartAlign::Right, ..text(&format!("{id}-value"), &format!("{label} value"), source, y, 158.0, 18.0) });
                parts.push(Part { kind: PartKind::Bar, color: colour, ..text(&format!("{id}-bar"), &format!("{label} bar"), source, y + 21.0, 158.0, 5.0) });
            }
            parts
        }
        WidgetKind::NowPlaying => vec![
            Part { kind: PartKind::Image, x: 12.0, stretch: false, radius: 8.0, ..text("cover", "Cover", "media.art", 12.0, 72.0, 72.0) },
            Part { x: 96.0, size: 15.0, weight: PartWeight::SemiBold, ..text("title", "Title", "media.title", 15.0, 210.0, 20.0) },
            Part { x: 96.0, color: PartColor::Muted, ..text("artist", "Artist", "media.artist", 38.0, 210.0, 18.0) },
            Part { kind: PartKind::Bar, x: 96.0, color: PartColor::Highlight, from_bottom: true, ..text("progress", "Progress", "media.progress", 73.0, 210.0, 5.0) },
        ],
    }
}

/// Whether these parts show anything from a family of live values
/// ("clock", "stats", "media"), directly or inside a text template.
pub fn parts_use(parts: &[Part], family: &str) -> bool {
    let (source, template) = (format!("{family}."), format!("{{{family}."));
    parts.iter().any(|part| !part.hidden && (part.source.starts_with(&source) || (part.source.is_empty() && part.text.contains(&template))))
}

/// The live values a part can show, for the settings app's "shows" list.
pub fn sources_for(kind: WidgetKind) -> Vec<(&'static str, &'static str)> {
    match kind {
        WidgetKind::Clock => vec![("clock.time", "Time"), ("clock.suffix", "AM / PM"), ("clock.date", "Date")],
        WidgetKind::SystemStats => vec![("stats.cpu", "CPU use"), ("stats.memory", "Memory use"), ("stats.gpu", "GPU use")],
        WidgetKind::NowPlaying => vec![("media.title", "Title"), ("media.artist", "Artist"), ("media.progress", "Progress"), ("media.art", "Cover")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [WidgetKind; 3] = [WidgetKind::Clock, WidgetKind::SystemStats, WidgetKind::NowPlaying];

    #[test]
    fn default_parts_are_valid_and_fit_their_widget() {
        for kind in KINDS {
            let parts = default_parts(kind);
            assert_eq!(sanitise_parts(parts.clone()), parts, "{kind:?}: the defaults need no tidying");
            let (width, height) = default_size(kind);
            for part in &parts {
                assert!(part.x >= 0.0 && part.y >= 0.0 && part.x + part.width <= width && part.y + part.height <= height, "{kind:?} {} is inside the widget", part.id);
                assert!(!part.name.is_empty());
            }
        }
    }

    #[test]
    fn parts_hold_colour_choices_never_colours() {
        let json = serde_json::to_string(&default_parts(WidgetKind::SystemStats)).unwrap();
        assert!(!json.contains('#'));
        assert!(json.contains(r#""color":"highlight""#) && json.contains(r#""color":"tertiary""#));
        assert_eq!(PartColor::Accent.role(), Some(Role::Accent));
        assert_eq!(PartColor::Muted.role(), None);
    }

    #[test]
    fn parts_follow_the_widget_when_it_is_resized() {
        let default = default_size(WidgetKind::NowPlaying);
        let parts = default_parts(WidgetKind::NowPlaying);
        let part = |id: &str| parts.iter().find(|p| p.id == id).unwrap();
        let bigger = (default.0 + 100.0, default.1 + 40.0);
        assert_eq!(part("cover").rect_in(bigger, default), (12.0, 12.0, 72.0, 72.0), "the cover stays put");
        assert_eq!(part("title").rect_in(bigger, default), (96.0, 15.0, 310.0, 20.0), "text grows with the width");
        assert_eq!(part("progress").rect_in(bigger, default), (96.0, 113.0, 310.0, 5.0), "the bar stays near the bottom");
        assert_eq!(part("title").rect_in((60.0, 96.0), default).2, 1.0, "never narrower than a pixel");
    }

    #[test]
    fn wild_parts_are_tamed_and_bad_ids_dropped() {
        let wild = Part { id: "a".into(), x: f32::NAN, width: -5.0, size: 900.0, opacity: 0, font: Some("   ".into()), ..Default::default() };
        let parts = sanitise_parts(vec![wild, Part { id: "a".into(), ..Default::default() }, Part { id: "has space".into(), ..Default::default() }, Part::default()]);
        assert_eq!(parts.len(), 1, "duplicates, odd ids and empty ids are dropped");
        assert_eq!((parts[0].x, parts[0].width, parts[0].size, parts[0].opacity), (0.0, 1.0, 200.0, 10));
        assert_eq!((parts[0].font.as_deref(), parts[0].name.as_str()), (None, "a"));
        assert_eq!(sanitise_parts((0..50).map(|n| Part { id: format!("p{n}"), ..Default::default() }).collect()).len(), MAX_PARTS);
    }

    #[test]
    fn parts_say_which_live_values_they_need() {
        let clock = default_parts(WidgetKind::Clock);
        assert!(parts_use(&clock, "clock") && !parts_use(&clock, "stats"));
        let mixed = vec![Part { id: "a".into(), text: "CPU {stats.cpu} at {clock.time}".into(), ..Default::default() }];
        assert!(parts_use(&mixed, "stats") && parts_use(&mixed, "clock") && !parts_use(&mixed, "media"));
        let hidden = vec![Part { id: "a".into(), source: "media.title".into(), hidden: true, ..Default::default() }];
        assert!(!parts_use(&hidden, "media"), "a hidden part needs nothing");
    }

    #[test]
    fn an_old_part_file_fills_in_defaults() {
        let part: Part = serde_json::from_str(r#"{ "id": "x", "kind": "bar", "source": "stats.cpu" }"#).unwrap();
        assert_eq!((part.kind, part.opacity, part.align, part.hidden), (PartKind::Bar, 100, PartAlign::Left, false));
    }
}
