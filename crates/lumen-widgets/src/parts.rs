//! Draws a widget from its parts: each line of text, level bar, ring, graph,
//! icon, shape and picture where the part says, in the font, size and colour
//! the part says. Colours are looked up in the [`Paint`], so they follow the
//! theme and the widget's component style.
//!
//! Parts can also react: a part with a condition is only drawn while it
//! holds, one with an alert changes colour while that holds, and one with a
//! click action says what a click on it does.

use lumen_ipc::{
    behaviour::{self, Action},
    parts::{Part, PartAlign, PartColor, PartKind, PartWeight},
    weather::WeatherState,
};
use lumen_render::{Bitmap, Canvas, Rect, Rgba, TextStyle, Weight};

use crate::{clock::ClockData, media::MediaInfo, paint::Paint, stats::StatsData};

/// Windows' own icon font, and the font that draws emoji in colour.
const ICON_FONT: &str = "Segoe Fluent Icons";
const EMOJI_FONT: &str = "Segoe UI Emoji";

/// How a value has moved lately: the newest reading last, each 0.0–1.0.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct History {
    pub cpu: Vec<f32>,
    pub memory: Vec<f32>,
    pub gpu: Vec<f32>,
}

impl History {
    /// The most readings kept (two minutes at one every two seconds).
    pub const LENGTH: usize = 60;

    pub fn push(&mut self, stats: &StatsData) {
        let add = |list: &mut Vec<f32>, value: u8| {
            list.push(f32::from(value) / 100.0);
            if list.len() > Self::LENGTH {
                list.remove(0);
            }
        };
        add(&mut self.cpu, stats.cpu);
        add(&mut self.memory, stats.memory);
        add(&mut self.gpu, stats.gpu.unwrap_or(0));
    }

    fn of(&self, source: &str) -> &[f32] {
        match source {
            "stats.cpu" => &self.cpu,
            "stats.memory" => &self.memory,
            "stats.gpu" => &self.gpu,
            _ => &[],
        }
    }
}

/// The live values parts can show.
pub struct Data<'a> {
    pub clock: &'a ClockData,
    pub stats: &'a StatsData,
    pub media: Option<&'a MediaInfo>,
    /// The cover of what is playing, if it has one.
    pub art: Option<&'a Bitmap>,
    pub weather: &'a WeatherState,
    pub history: &'a History,
    /// Whether the mouse is over the widget (for parts shown only then).
    pub hovered: bool,
}

/// Whether a part is on screen right now: not hidden, its condition holds,
/// and the mouse is over the widget if it only shows then.
fn visible(part: &Part, data: &Data<'_>) -> bool {
    !part.hidden && (!part.hover_only || data.hovered) && behaviour::holds(&part.when, &|name| value_of(name, data))
}

/// Draws the card and every visible part. `default_size` is the widget size
/// the parts' positions were written for.
pub fn draw(canvas: &Canvas<'_>, paint: &Paint, parts: &[Part], data: &Data<'_>, size: (f32, f32), default_size: (f32, f32)) {
    paint.card(canvas, size.0, size.1);
    for part in parts.iter().filter(|part| visible(part, data)) {
        let (x, y, width, height) = part.rect_in(size, default_size);
        let rect = Rect::new(x, y, width, height);
        let colour = colour(paint, part, data);
        match part.kind {
            PartKind::Text => {
                let text = text_for(part, data);
                let family = part.font.as_deref().unwrap_or(&paint.body_font);
                let weight = match part.weight {
                    PartWeight::Regular => Weight::Regular,
                    PartWeight::Medium => Weight::Medium,
                    PartWeight::SemiBold => Weight::SemiBold,
                };
                let style = TextStyle { family, size: part.size * paint.text_scale, weight, color: colour, align_right: part.align == PartAlign::Right, align_center: part.align == PartAlign::Centre };
                canvas.text(&text, rect, style);
            }
            PartKind::Icon => {
                // A named icon comes from Windows' icon font; anything else is drawn as typed (an emoji, a letter).
                let (text, family) = match behaviour::icon_glyph(&part.text) {
                    Some(glyph) => (glyph.to_string(), ICON_FONT),
                    None => (text_for(part, data), EMOJI_FONT),
                };
                let side = width.min(height);
                let style = TextStyle { family, size: side * 0.8, weight: Weight::Regular, color: colour, align_right: false, align_center: true };
                // Centred in its box, whatever shape the box is.
                canvas.text(&text, Rect::new(x, y + (height - side) / 2.0, width, side), style);
            }
            PartKind::Bar => {
                // A value that does not exist right now (a track of unknown length) has no bar.
                if let Some(fraction) = fraction_for(&part.source, data) {
                    paint.bar(canvas, rect, fraction, colour);
                }
            }
            PartKind::Ring => {
                if let Some(fraction) = fraction_for(&part.source, data) {
                    // The line is a seventh of the ring's width, and sits inside the box.
                    let side = width.min(height);
                    let stroke = (side / 7.0).clamp(2.0, 14.0);
                    let (centre, radius) = ((x + width / 2.0, y + height / 2.0), (side - stroke) / 2.0);
                    canvas.arc(centre, radius, 0.0, 360.0, stroke, paint.track);
                    canvas.arc(centre, radius, 0.0, 360.0 * fraction.clamp(0.0, 1.0), stroke, colour);
                }
            }
            PartKind::Graph => {
                let readings = data.history.of(&part.source);
                // A faint line along the bottom, so an empty graph still has a place.
                canvas.fill_round_rect(Rect::new(x, y + height - 1.0, width, 1.0), 0.0, paint.track);
                if readings.len() >= 2 {
                    // The newest reading is at the right edge; older ones step left.
                    let step = width / (History::LENGTH - 1) as f32;
                    let first_x = x + width - step * (readings.len() - 1) as f32;
                    let points: Vec<(f32, f32)> = readings.iter().enumerate().map(|(index, value)| (first_x + step * index as f32, y + 1.0 + (height - 2.0) * (1.0 - value.clamp(0.0, 1.0)))).collect();
                    canvas.polyline(&points, 1.6, colour);
                }
            }
            PartKind::Shape => canvas.fill_round_rect(rect, part.radius.min(width.min(height) / 2.0), colour),
            PartKind::Image => {
                let radius = part.radius.min(width.min(height) / 2.0);
                match data.art.filter(|_| part.source == "media.art") {
                    Some(bitmap) => canvas.image(bitmap, rect, radius),
                    // No picture: a soft gradient in the theme's colours.
                    None => canvas.fill_round_rect_gradient(rect, radius, paint.primary.with_alpha(colour.a), paint.tertiary.with_alpha(colour.a)),
                }
            }
        }
    }
}

/// What a click at a point inside the widget does: the action of the
/// topmost visible part there that has one.
pub fn action_at(parts: &[Part], data: &Data<'_>, size: (f32, f32), default_size: (f32, f32), point: (f32, f32)) -> Option<Action> {
    parts.iter().rev().filter(|part| visible(part, data)).find_map(|part| {
        let (x, y, width, height) = part.rect_in(size, default_size);
        // A few pixels of grace: text boxes are often only as tall as the text.
        let inside = point.0 >= x - 3.0 && point.0 < x + width + 3.0 && point.1 >= y - 3.0 && point.1 < y + height + 3.0;
        inside.then(|| behaviour::parse_action(&part.click)).flatten()
    })
}

/// Whether any part could ever take a click (the widget then takes the mouse).
pub fn any_clickable(parts: &[Part]) -> bool {
    parts.iter().any(|part| !part.hidden && behaviour::parse_action(&part.click).is_some())
}

/// Whether any part shows only while the mouse is over the widget.
pub fn any_hover_only(parts: &[Part]) -> bool {
    parts.iter().any(|part| !part.hidden && part.hover_only)
}

fn colour(paint: &Paint, part: &Part, data: &Data<'_>) -> Rgba {
    // While its alert holds, a part takes the alert colour instead of its own.
    let alerting = !part.alert_when.trim().is_empty() && behaviour::holds(&part.alert_when, &|name| value_of(name, data));
    let base = match if alerting { part.alert_color } else { part.color } {
        PartColor::Text => paint.text,
        PartColor::Muted => paint.text_muted,
        PartColor::Highlight => paint.primary,
        other => other.role().map_or(paint.text, |role| paint.role(role)),
    };
    base.with_alpha(base.a * f32::from(part.opacity) / 100.0)
}

/// What a text part says right now: its live value, or its own text with
/// any `{live.value}` in it filled in.
fn text_for(part: &Part, data: &Data<'_>) -> String {
    if part.source.is_empty() { fill(&part.text, data) } else { value_of(&part.source, data).unwrap_or_default() }
}

/// A live value as text; `None` if there is no value of that name.
pub fn value_of(source: &str, data: &Data<'_>) -> Option<String> {
    let percent = |value: Option<u8>| value.map_or("n/a".to_owned(), |v| format!("{v}%"));
    let yes = |holds: bool| if holds { "yes".to_owned() } else { String::new() };
    Some(match source {
        "clock.time" => data.clock.time.clone(),
        "clock.suffix" => data.clock.suffix.clone(),
        "clock.date" => data.clock.date.clone(),
        "stats.cpu" => percent(Some(data.stats.cpu)),
        "stats.memory" => percent(Some(data.stats.memory)),
        "stats.gpu" => percent(data.stats.gpu),
        "media.title" => data.media.map_or("Nothing playing".to_owned(), |media| media.title.clone()),
        "media.artist" => data.media.map_or("Start some music or a video".to_owned(), |media| media.artist.clone()),
        "media.progress" => data.media.and_then(MediaInfo::progress).map_or(String::new(), |p| format!("{}%", (p * 100.0).round() as u8)),
        // For conditions: "yes" while something is playing (or loaded), nothing otherwise.
        "media.playing" => yes(data.media.is_some_and(|media| media.playing)),
        "media.active" => yes(data.media.is_some()),
        other => return crate::weather::value_of(other, data.weather),
    })
}

/// Replaces every `{name}` that names a live value; anything else in braces is left as typed.
fn fill(template: &str, data: &Data<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| value_of(after[..close].trim(), data).map(|value| (close, value))) {
            Some((close, value)) => {
                out.push_str(&value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// How full a bar or ring is, 0.0–1.0; `None` if there is nothing to show.
fn fraction_for(source: &str, data: &Data<'_>) -> Option<f32> {
    let percent = |value: u8| f32::from(value) / 100.0;
    match source {
        "stats.cpu" => Some(percent(data.stats.cpu)),
        "stats.memory" => Some(percent(data.stats.memory)),
        "stats.gpu" => Some(percent(data.stats.gpu.unwrap_or(0))),
        "media.progress" => data.media.and_then(MediaInfo::progress),
        // Any other value that is a percentage (humidity) fills by that much.
        other => Some(value_of(other, data).filter(|value| value.trim_end().ends_with('%')).and_then(|value| behaviour::number_in(&value)).map_or(0.0, |n| (n / 100.0).clamp(0.0, 1.0) as f32)),
    }
}

#[cfg(test)]
mod tests {
    use lumen_ipc::{parts::default_parts, widgets::WidgetKind};
    use lumen_theme::{Color, Mode, Theme, Variant};

    use super::*;

    fn data<'a>(clock: &'a ClockData, stats: &'a StatsData) -> Data<'a> {
        Data { clock, stats, media: None, art: None, weather: Box::leak(Box::default()), history: Box::leak(Box::default()), hovered: false }
    }

    fn clock() -> ClockData {
        ClockData { date: "Saturday 14 Nov".into(), time: "9:45".into(), suffix: "AM".into() }
    }

    #[test]
    fn text_parts_show_their_live_value_or_their_own_text() {
        let (clock, stats) = (clock(), StatsData { cpu: 12, memory: 41, gpu: None });
        let data = data(&clock, &stats);
        let said = |kind: WidgetKind, id: &str| text_for(default_parts(kind).iter().find(|p| p.id == id).unwrap(), &data);
        assert_eq!(said(WidgetKind::Clock, "time"), "9:45");
        assert_eq!(said(WidgetKind::Clock, "suffix"), "AM");
        assert_eq!(said(WidgetKind::SystemStats, "title"), "System", "fixed text");
        assert_eq!(said(WidgetKind::SystemStats, "cpu-value"), "12%");
        assert_eq!(said(WidgetKind::SystemStats, "gpu-value"), "n/a");
        assert_eq!(said(WidgetKind::NowPlaying, "title"), "Nothing playing");
    }

    #[test]
    fn text_can_mix_fixed_words_and_live_values() {
        let (clock, stats) = (clock(), StatsData { cpu: 12, memory: 41, gpu: None });
        let data = data(&clock, &stats);
        assert_eq!(fill("CPU {stats.cpu} · {clock.time} {clock.suffix}", &data), "CPU 12% · 9:45 AM");
        assert_eq!(fill("{ stats.memory } used", &data), "41% used", "spaces inside the braces are fine");
        assert_eq!(fill("{nope} and {unclosed", &data), "{nope} and {unclosed", "anything else is left as typed");
        assert_eq!(fill("plain", &data), "plain");
        assert_eq!(fill("é{clock.time}é", &data), "é9:45é");
        assert_eq!(fill("{weather.condition}, {weather.temp}", &data), "Weather is off, –", "weather values work in text too");
    }

    #[test]
    fn bars_and_rings_follow_their_value_and_vanish_when_there_is_none() {
        let (clock, stats) = (ClockData { date: String::new(), time: String::new(), suffix: String::new() }, StatsData { cpu: 50, memory: 100, gpu: None });
        let data = data(&clock, &stats);
        assert_eq!(fraction_for("stats.cpu", &data), Some(0.5));
        assert_eq!(fraction_for("stats.gpu", &data), Some(0.0), "no reading: an empty bar");
        assert_eq!(fraction_for("media.progress", &data), None, "nothing playing: no bar");
        assert_eq!(fraction_for("clock.time", &data), Some(0.0), "a value that is not a percentage is empty");
    }

    #[test]
    fn part_colours_come_from_the_paint_and_change_while_an_alert_holds() {
        let theme = Theme::from_color(Color::new(0x2F, 0x5B, 0xEA), Variant::Tonal, Mode::Dark, 0).unwrap();
        let paint = Paint::from_theme(&theme);
        let (clock, stats) = (clock(), StatsData { cpu: 95, memory: 41, gpu: None });
        let data = data(&clock, &stats);
        let part = |color: PartColor, opacity: u8| Part { color, opacity, ..Default::default() };
        assert_eq!(colour(&paint, &part(PartColor::Text, 100), &data), paint.text);
        assert_eq!(colour(&paint, &part(PartColor::Highlight, 100), &data), paint.primary);
        assert_eq!(colour(&paint, &part(PartColor::Tertiary, 100), &data), paint.tertiary);
        assert_eq!(colour(&paint, &part(PartColor::Primary, 50), &data), paint.primary.with_alpha(0.5));

        let hot = Part { color: PartColor::Highlight, alert_when: "stats.cpu > 90".into(), alert_color: PartColor::Accent, ..Default::default() };
        assert_eq!(colour(&paint, &hot, &data), paint.accent, "the CPU is at 95: the alert colour");
        let calm = Part { alert_when: "stats.memory > 90".into(), ..hot };
        assert_eq!(colour(&paint, &calm, &data), paint.primary, "memory is at 41: its own colour");
    }

    #[test]
    fn conditions_and_hover_decide_what_is_drawn() {
        let (clock, stats) = (clock(), StatsData { cpu: 95, memory: 41, gpu: None });
        let mut data = data(&clock, &stats);
        let when = |condition: &str| Part { when: condition.into(), ..Default::default() };
        assert!(visible(&when("stats.cpu > 90"), &data) && !visible(&when("stats.cpu < 90"), &data));
        assert!(!visible(&when("media.playing"), &data) && visible(&when("!media.playing"), &data), "nothing is playing");
        assert!(visible(&when(""), &data) && !visible(&Part { hidden: true, ..Default::default() }, &data));
        let hover = Part { hover_only: true, ..Default::default() };
        assert!(!visible(&hover, &data));
        data.hovered = true;
        assert!(visible(&hover, &data));
    }

    #[test]
    fn a_click_finds_the_topmost_visible_part_with_an_action() {
        let (clock, stats) = (clock(), StatsData { cpu: 10, memory: 41, gpu: None });
        let data = data(&clock, &stats);
        let part = |id: &str, x: f32, click: &str, when: &str| Part { id: id.into(), x, y: 10.0, width: 40.0, height: 20.0, click: click.into(), when: when.into(), ..Default::default() };
        let parts = vec![part("under", 10.0, "lumen.open", ""), part("over", 30.0, "media.toggle", ""), part("plain", 100.0, "", ""), part("gone", 160.0, "lumen.open", "stats.cpu > 90")];
        let at = |x: f32, y: f32| action_at(&parts, &data, (220.0, 110.0), (220.0, 110.0), (x, y));
        assert_eq!(at(15.0, 20.0), Some(Action::OpenLumen));
        assert_eq!(at(40.0, 20.0), Some(Action::ToggleMedia), "where two overlap, the one drawn on top");
        assert_eq!(at(110.0, 20.0), None, "a part with no action");
        assert_eq!(at(170.0, 20.0), None, "a part whose condition does not hold cannot be clicked");
        assert_eq!(at(15.0, 80.0), None, "empty space");
        assert!(any_clickable(&parts) && !any_clickable(&parts[2..3]) && !any_hover_only(&parts));
    }

    #[test]
    fn history_keeps_the_latest_readings() {
        let mut history = History::default();
        for cpu in 0..100u8 {
            history.push(&StatsData { cpu, memory: 50, gpu: None });
        }
        assert_eq!(history.cpu.len(), History::LENGTH);
        assert_eq!((history.of("stats.cpu").last().copied(), history.of("stats.cpu").first().copied()), (Some(0.99), Some(0.40)));
        assert_eq!((history.of("stats.gpu").last().copied(), history.of("clock.time").len()), (Some(0.0), 0));
    }
}
