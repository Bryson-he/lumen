//! Draws a widget from its parts: each line of text, level bar and picture
//! where the part says, in the font, size and colour the part says. Colours
//! are looked up in the [`Paint`], so they follow the theme and the widget's
//! component style.

use lumen_ipc::parts::{Part, PartAlign, PartColor, PartKind, PartWeight};
use lumen_render::{Bitmap, Canvas, Rect, Rgba, TextStyle, Weight};

use crate::{clock::ClockData, media::MediaInfo, paint::Paint, stats::StatsData};

/// The live values parts can show.
pub struct Data<'a> {
    pub clock: &'a ClockData,
    pub stats: &'a StatsData,
    pub media: Option<&'a MediaInfo>,
    /// The cover of what is playing, if it has one.
    pub art: Option<&'a Bitmap>,
}

/// Draws the card and every visible part. `default_size` is the widget size
/// the parts' positions were written for.
pub fn draw(canvas: &Canvas<'_>, paint: &Paint, parts: &[Part], data: &Data<'_>, size: (f32, f32), default_size: (f32, f32)) {
    paint.card(canvas, size.0, size.1);
    for part in parts.iter().filter(|part| !part.hidden) {
        let (x, y, width, height) = part.rect_in(size, default_size);
        let rect = Rect::new(x, y, width, height);
        let colour = colour(paint, part);
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
            PartKind::Bar => {
                // A value that does not exist right now (a track of unknown length) has no bar.
                if let Some(fraction) = fraction_for(&part.source, data) {
                    paint.bar(canvas, rect, fraction, colour);
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

fn colour(paint: &Paint, part: &Part) -> Rgba {
    let base = match part.color {
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
fn value_of(source: &str, data: &Data<'_>) -> Option<String> {
    let percent = |value: Option<u8>| value.map_or("n/a".to_owned(), |v| format!("{v}%"));
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
        _ => return None,
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

/// How full a bar is, 0.0–1.0; `None` if there is nothing to show.
fn fraction_for(source: &str, data: &Data<'_>) -> Option<f32> {
    let percent = |value: u8| f32::from(value) / 100.0;
    match source {
        "stats.cpu" => Some(percent(data.stats.cpu)),
        "stats.memory" => Some(percent(data.stats.memory)),
        "stats.gpu" => Some(percent(data.stats.gpu.unwrap_or(0))),
        "media.progress" => data.media.and_then(MediaInfo::progress),
        _ => Some(0.0),
    }
}

#[cfg(test)]
mod tests {
    use lumen_ipc::{parts::default_parts, widgets::WidgetKind};
    use lumen_theme::{Color, Mode, Theme, Variant};

    use super::*;

    fn data<'a>(clock: &'a ClockData, stats: &'a StatsData) -> Data<'a> {
        Data { clock, stats, media: None, art: None }
    }

    #[test]
    fn text_parts_show_their_live_value_or_their_own_text() {
        let clock = ClockData { date: "Saturday 14 Nov".into(), time: "9:45".into(), suffix: "AM".into() };
        let stats = StatsData { cpu: 12, memory: 41, gpu: None };
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
        let clock = ClockData { date: "Saturday 14 Nov".into(), time: "9:45".into(), suffix: "AM".into() };
        let stats = StatsData { cpu: 12, memory: 41, gpu: None };
        let data = data(&clock, &stats);
        assert_eq!(fill("CPU {stats.cpu} · {clock.time} {clock.suffix}", &data), "CPU 12% · 9:45 AM");
        assert_eq!(fill("{ stats.memory } used", &data), "41% used", "spaces inside the braces are fine");
        assert_eq!(fill("{nope} and {unclosed", &data), "{nope} and {unclosed", "anything else is left as typed");
        assert_eq!(fill("plain", &data), "plain");
        assert_eq!(fill("é{clock.time}é", &data), "é9:45é");
    }

    #[test]
    fn bars_follow_their_value_and_vanish_when_there_is_none() {
        let clock = ClockData { date: String::new(), time: String::new(), suffix: String::new() };
        let stats = StatsData { cpu: 50, memory: 100, gpu: None };
        let data = data(&clock, &stats);
        assert_eq!(fraction_for("stats.cpu", &data), Some(0.5));
        assert_eq!(fraction_for("stats.gpu", &data), Some(0.0), "no reading: an empty bar");
        assert_eq!(fraction_for("media.progress", &data), None, "nothing playing: no bar");
    }

    #[test]
    fn part_colours_come_from_the_paint() {
        let theme = Theme::from_color(Color::new(0x2F, 0x5B, 0xEA), Variant::Tonal, Mode::Dark, 0).unwrap();
        let paint = Paint::from_theme(&theme);
        let part = |color: PartColor, opacity: u8| Part { color, opacity, ..Default::default() };
        assert_eq!(colour(&paint, &part(PartColor::Text, 100)), paint.text);
        assert_eq!(colour(&paint, &part(PartColor::Highlight, 100)), paint.primary);
        assert_eq!(colour(&paint, &part(PartColor::Tertiary, 100)), paint.tertiary);
        assert_eq!(colour(&paint, &part(PartColor::Primary, 50)), paint.primary.with_alpha(0.5));
        assert!((colour(&paint, &part(PartColor::Muted, 50)).a - paint.text_muted.a / 2.0).abs() < 1e-6);
    }
}
