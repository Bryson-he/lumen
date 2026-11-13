//! The status bar: lays out the modules in its left, centre and right slots
//! and draws them. Colours and fonts come from the theme's [`Paint`], as for
//! widgets.

use std::collections::HashMap;

use lumen_ipc::bar::{BarConfig, BarModule, BarPosition, LauncherApp};
use lumen_render::{Bitmap, Canvas, Rect, Rgba, TextStyle, Weight};

use crate::{clock::ClockData, media::MediaInfo, paint::Paint, stats::StatsData};

/// Space between the bar's ends and its first and last modules.
const EDGE: f32 = 16.0;
/// Space between neighbouring modules.
const GAP: f32 = 20.0;
const TEXT_SIZE: f32 = 13.0;
/// The widest a "now playing" module may grow before its text is cut short.
const MEDIA_MAX: f32 = 320.0;
/// Space between launcher icons.
const ICON_GAP: f32 = 10.0;

/// What a click on part of the bar does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenSettings,
    ToggleMedia,
    Launch(String),
}

/// A clickable stretch of the bar, in logical pixels from its left end.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub x: f32,
    pub width: f32,
    pub action: Action,
}

/// The live values the modules show.
pub struct Data<'a> {
    pub clock: &'a ClockData,
    pub stats: &'a StatsData,
    pub media: Option<&'a MediaInfo>,
}

/// One module, measured and ready to place.
enum Piece<'a> {
    /// A run of text in one style.
    Text {
        text: String,
        weight: Weight,
        color: Rgba,
        width: f32,
        action: Option<Action>,
    },
    /// A muted label followed by a value: "CPU 12%".
    Stat {
        label: &'static str,
        value: String,
        label_width: f32,
        width: f32,
    },
    /// A dot (lit while playing) and "Title · Artist".
    Media {
        text: String,
        playing: bool,
        width: f32,
    },
    Launcher {
        apps: &'a [LauncherApp],
        icon: f32,
        width: f32,
    },
}

impl Piece<'_> {
    fn width(&self) -> f32 {
        match self {
            Self::Text { width, .. } | Self::Stat { width, .. } | Self::Media { width, .. } | Self::Launcher { width, .. } => *width,
        }
    }
}

/// Draws the bar and returns its clickable regions.
pub fn draw(canvas: &Canvas<'_>, paint: &Paint, config: &BarConfig, data: &Data<'_>, icons: &HashMap<String, Bitmap>, width: f32, height: f32) -> Vec<Region> {
    // Background, with a hairline on the edge that faces the desktop.
    // A bar with its own strip of the screen has only wallpaper behind it, so
    // it can be glass. A floating one sits over windows and must stay readable.
    let background = if config.reserve_space { paint.glass } else { paint.surface };
    canvas.fill_round_rect(Rect::new(0.0, 0.0, width, height), 0.0, background);
    let line_y = if config.position == BarPosition::Top { height - 1.0 } else { 0.0 };
    canvas.fill_round_rect(Rect::new(0.0, line_y, width, 1.0), 0.0, paint.border);

    fn measure<'a>(canvas: &Canvas<'_>, paint: &Paint, slot: &'a [BarModule], data: &Data<'_>, height: f32) -> Vec<Piece<'a>> {
        slot.iter().filter_map(|module| piece(canvas, paint, module, data, height)).collect()
    }
    let total = |pieces: &[Piece<'_>]| pieces.iter().map(Piece::width).sum::<f32>() + GAP * pieces.len().saturating_sub(1) as f32;
    let left = measure(canvas, paint, &config.left, data, height);
    let centre = measure(canvas, paint, &config.centre, data, height);
    let right = measure(canvas, paint, &config.right, data, height);

    let mut regions = Vec::new();
    let mut place = |pieces: &[Piece<'_>], mut x: f32| {
        for item in pieces {
            draw_piece(canvas, paint, item, x, height, icons, &mut regions);
            x += item.width() + GAP;
        }
    };
    place(&left, EDGE);
    place(&right, width - EDGE - total(&right));
    // The centre slot is centred on the bar, but never runs under the left slot.
    let centre_start = ((width - total(&centre)) / 2.0).max(EDGE + total(&left) + GAP);
    place(&centre, centre_start);
    regions
}

/// Whether any module shows the time or date, system stats, or what is playing.
pub fn needs(config: &BarConfig) -> (bool, bool, bool) {
    let has = |wanted: fn(&BarModule) -> bool| config.modules().any(wanted);
    (has(|m| matches!(m, BarModule::Clock | BarModule::Date)), has(|m| matches!(m, BarModule::Cpu | BarModule::Memory | BarModule::Gpu)), has(|m| matches!(m, BarModule::Media)))
}

/// The paths of every launcher app in the bar.
pub fn launcher_paths(config: &BarConfig) -> Vec<&str> {
    config.modules().filter_map(|m| if let BarModule::Launcher { apps } = m { Some(apps) } else { None }).flatten().map(|app| app.path.as_str()).collect()
}

fn body(paint: &Paint, weight: Weight, color: Rgba) -> TextStyle<'_> {
    paint.body(TEXT_SIZE, weight, color)
}

/// Measures one module. `None` if it has nothing to show right now.
fn piece<'a>(canvas: &Canvas<'_>, paint: &Paint, module: &'a BarModule, data: &Data<'_>, height: f32) -> Option<Piece<'a>> {
    let text = |text: String, weight: Weight, action: Option<Action>| {
        let width = canvas.text_width(&text, body(paint, weight, paint.text));
        Piece::Text { text, weight, color: paint.text, width, action }
    };
    let stat = |label: &'static str, value: Option<u8>| {
        let value = value.map_or("n/a".to_owned(), |v| format!("{v}%"));
        let label_width = canvas.text_width(label, body(paint, Weight::Regular, paint.text_muted)) + 5.0;
        // Sized for three digits, so the bar does not shuffle as values change.
        let value_width = canvas.text_width("100%", body(paint, Weight::Medium, paint.text));
        Piece::Stat { label, value, label_width, width: label_width + value_width }
    };
    Some(match module {
        BarModule::Lumen => text("Lumen".to_owned(), Weight::SemiBold, Some(Action::OpenSettings)),
        BarModule::Clock => {
            let shown = if data.clock.suffix.is_empty() { data.clock.time.clone() } else { format!("{} {}", data.clock.time, data.clock.suffix) };
            text(shown, Weight::Medium, None)
        }
        BarModule::Date => text(data.clock.date.clone(), Weight::Regular, None),
        BarModule::Cpu => stat("CPU", Some(data.stats.cpu)),
        BarModule::Memory => stat("RAM", Some(data.stats.memory)),
        BarModule::Gpu => stat("GPU", data.stats.gpu),
        BarModule::Media => {
            let media = data.media?;
            let shown = if media.artist.is_empty() { media.title.clone() } else { format!("{} · {}", media.title, media.artist) };
            let width = (canvas.text_width(&shown, body(paint, Weight::Regular, paint.text)) + 14.0).min(MEDIA_MAX);
            Piece::Media { text: shown, playing: media.playing, width }
        }
        BarModule::Launcher { apps } => {
            if apps.is_empty() {
                return None;
            }
            let icon = (height - 14.0).clamp(14.0, 32.0);
            Piece::Launcher { apps, icon, width: apps.len() as f32 * icon + (apps.len() - 1) as f32 * ICON_GAP }
        }
    })
}

fn draw_piece(canvas: &Canvas<'_>, paint: &Paint, piece: &Piece<'_>, x: f32, height: f32, icons: &HashMap<String, Bitmap>, regions: &mut Vec<Region>) {
    // One line of 13 px text is about 18 px tall.
    let text_y = (height - 18.0) / 2.0;
    let line = |x: f32, width: f32| Rect::new(x, text_y, width + 2.0, 18.0);
    match piece {
        Piece::Text { text, weight, color, width, action } => {
            canvas.text(text, line(x, *width), body(paint, *weight, *color));
            if let Some(action) = action {
                regions.push(Region { x: x - GAP / 2.0, width: width + GAP, action: action.clone() });
            }
        }
        Piece::Stat { label, value, label_width, width } => {
            canvas.text(label, line(x, *label_width), body(paint, Weight::Regular, paint.text_muted));
            canvas.text(value, line(x + label_width, width - label_width), body(paint, Weight::Medium, paint.text));
        }
        Piece::Media { text, playing, width } => {
            let dot = if *playing { paint.primary } else { paint.track };
            canvas.fill_round_rect(Rect::new(x, height / 2.0 - 3.0, 6.0, 6.0), 3.0, dot);
            canvas.text(text, line(x + 14.0, width - 14.0), body(paint, Weight::Regular, paint.text));
            regions.push(Region { x: x - GAP / 2.0, width: width + GAP, action: Action::ToggleMedia });
        }
        Piece::Launcher { apps, icon, .. } => {
            let y = (height - icon) / 2.0;
            for (index, app) in apps.iter().enumerate() {
                let icon_x = x + index as f32 * (icon + ICON_GAP);
                let square = Rect::new(icon_x, y, *icon, *icon);
                match icons.get(&app.path) {
                    Some(bitmap) => canvas.image(bitmap, square, 0.0),
                    // No icon (the file moved, perhaps): a plain tile with its initial.
                    None => {
                        canvas.fill_round_rect(square, 5.0, paint.track);
                        let initial: String = app.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                        canvas.text(&initial, Rect::new(icon_x + icon / 2.0 - 4.5, y + icon / 2.0 - 9.0, 14.0, 18.0), body(paint, Weight::SemiBold, paint.text));
                    }
                }
                regions.push(Region { x: icon_x - ICON_GAP / 2.0, width: icon + ICON_GAP, action: Action::Launch(app.path.clone()) });
            }
        }
    }
}

/// The action under a point `x` logical pixels from the bar's left end.
pub fn action_at(regions: &[Region], x: f32) -> Option<&Action> {
    regions.iter().find(|region| x >= region.x && x < region.x + region.width).map(|region| &region.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_declare_the_data_they_need() {
        let mut config = BarConfig { left: vec![BarModule::Lumen], centre: vec![], right: vec![], ..Default::default() };
        assert_eq!(needs(&config), (false, false, false), "a bar with only the name never needs redrawing");
        config.right = vec![BarModule::Gpu, BarModule::Date];
        assert_eq!(needs(&config), (true, true, false));
        config.centre = vec![BarModule::Media];
        assert_eq!(needs(&config), (true, true, true));
    }

    #[test]
    fn launcher_paths_are_collected_from_every_slot() {
        let app = |name: &str| LauncherApp { name: name.into(), path: format!(r"C:\Apps\{name}.exe") };
        let config = BarConfig {
            left: vec![BarModule::Launcher { apps: vec![app("a"), app("b")] }],
            centre: vec![BarModule::Clock],
            right: vec![BarModule::Launcher { apps: vec![app("c")] }],
            ..Default::default()
        };
        assert_eq!(launcher_paths(&config), [r"C:\Apps\a.exe", r"C:\Apps\b.exe", r"C:\Apps\c.exe"]);
    }

    #[test]
    fn clicks_find_the_region_they_land_in() {
        let regions = [Region { x: 6.0, width: 60.0, action: Action::OpenSettings }, Region { x: 300.0, width: 32.0, action: Action::Launch("x".into()) }];
        assert_eq!(action_at(&regions, 20.0), Some(&Action::OpenSettings));
        assert_eq!(action_at(&regions, 66.0), None, "just past the end");
        assert_eq!(action_at(&regions, 310.0), Some(&Action::Launch("x".into())));
        assert_eq!(action_at(&regions, 150.0), None, "empty bar between modules");
    }
}
