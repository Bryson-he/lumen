//! The volume flyout: its size, where it goes, and how it is drawn. Colours
//! and fonts come from the theme's [`Paint`], as for widgets and the bar.

use lumen_ipc::flyout::{FlyoutPosition, FlyoutStyle};
use lumen_render::{Canvas, Rect, Weight};
use lumen_win::audio::Level;

use crate::{media::MediaInfo, paint::Paint};

/// Space between the flyout and the edges of the screen's work area.
pub const MARGIN: f32 = 24.0;
const PADDING: f32 = 20.0;
const TEXT_SIZE: f32 = 13.0;
/// One line of 13 px text is about 18 px tall.
const LINE: f32 = 18.0;
const ROW: f32 = 52.0;
const TRACK: f32 = 6.0;

/// Width and height in logical pixels.
pub fn size(style: FlyoutStyle) -> (f32, f32) {
    match style {
        FlyoutStyle::Pill => (300.0, ROW),
        FlyoutStyle::Card => (320.0, 92.0),
    }
}

/// The flyout's top-left corner, given the work area it sits in and its own
/// size and margin (all in the same units).
pub fn origin(position: FlyoutPosition, work: (i32, i32, i32, i32), size: (i32, i32), margin: i32) -> (i32, i32) {
    use FlyoutPosition::{BottomCentre, BottomLeft, BottomRight, TopCentre, TopLeft, TopRight};
    let (x, y, width, height) = work;
    let left = match position {
        TopLeft | BottomLeft => x + margin,
        TopCentre | BottomCentre => x + (width - size.0) / 2,
        TopRight | BottomRight => x + width - size.0 - margin,
    };
    let top = match position {
        TopLeft | TopCentre | TopRight => y + margin,
        BottomLeft | BottomCentre | BottomRight => y + height - size.1 - margin,
    };
    (left, top)
}

pub fn draw(canvas: &Canvas<'_>, paint: &Paint, style: FlyoutStyle, level: Level, media: Option<&MediaInfo>, width: f32, height: f32) {
    // A flyout appears over windows, so it is solid rather than glass, and
    // the pill is fully rounded whatever the theme's corner radius.
    let mut card = paint.clone();
    card.glass = paint.surface;
    if style == FlyoutStyle::Pill {
        card.radius = height / 2.0;
    }
    card.card(canvas, width, height);

    // Label, level, percentage.
    let label = if level.muted { "Muted" } else { "Volume" };
    let label_style = || paint.body(TEXT_SIZE, Weight::Regular, paint.text_muted);
    // Sized for the longer label and three digits, so nothing shifts as they change.
    let label_width = canvas.text_width("Volume", label_style()) + 2.0;
    let value_width = canvas.text_width("100%", paint.body(TEXT_SIZE, Weight::Medium, paint.text)) + 2.0;
    let text_y = (ROW - LINE) / 2.0;
    canvas.text(label, Rect::new(PADDING, text_y, label_width, LINE), label_style());

    let mut value = paint.body(TEXT_SIZE, Weight::Medium, if level.muted { paint.text_muted } else { paint.text });
    value.align_right = true;
    canvas.text(&format!("{}%", level.percent), Rect::new(width - PADDING - value_width, text_y, value_width, LINE), value);

    let track_x = PADDING + label_width + 14.0;
    let track = Rect::new(track_x, (ROW - TRACK) / 2.0, width - PADDING - value_width - 14.0 - track_x, TRACK);
    let fill = if level.muted { paint.text_muted } else { paint.primary };
    paint.bar(canvas, track, f32::from(level.percent) / 100.0, fill);

    if style == FlyoutStyle::Card {
        let y = ROW - 4.0;
        canvas.fill_round_rect(Rect::new(PADDING, y, width - PADDING * 2.0, 1.0), 0.0, paint.border);
        let line_y = y + (height - y - LINE) / 2.0;
        let (text, colour, dot) = match media {
            Some(media) => {
                let shown = if media.artist.is_empty() { media.title.clone() } else { format!("{} · {}", media.title, media.artist) };
                (shown, paint.text, if media.playing { paint.primary } else { paint.track })
            }
            None => ("Nothing playing".to_owned(), paint.text_muted, paint.track),
        };
        canvas.fill_round_rect(Rect::new(PADDING, line_y + LINE / 2.0 - 3.0, 6.0, 6.0), 3.0, dot);
        canvas.text(&text, Rect::new(PADDING + 14.0, line_y, width - PADDING * 2.0 - 14.0, LINE), paint.body(TEXT_SIZE, Weight::Regular, colour));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_keep_the_margin_from_the_work_area() {
        // A work area that starts below a 54 px bar on a 3840 x 2160 screen.
        let work = (0, 54, 3840, 2106);
        let size = (450, 78);
        assert_eq!(origin(FlyoutPosition::TopLeft, work, size, 36), (36, 90));
        assert_eq!(origin(FlyoutPosition::TopCentre, work, size, 36), (1695, 90));
        assert_eq!(origin(FlyoutPosition::TopRight, work, size, 36), (3354, 90));
        assert_eq!(origin(FlyoutPosition::BottomLeft, work, size, 36), (36, 2046));
        assert_eq!(origin(FlyoutPosition::BottomCentre, work, size, 36), (1695, 2046));
        assert_eq!(origin(FlyoutPosition::BottomRight, work, size, 36), (3354, 2046));
    }

    #[test]
    fn positions_follow_a_monitor_that_is_not_at_the_origin() {
        assert_eq!(origin(FlyoutPosition::TopLeft, (645, -2160, 3840, 2160), (300, 52), 24), (669, -2136));
    }

    #[test]
    fn the_card_is_taller_than_the_pill() {
        assert!(size(FlyoutStyle::Card).1 > size(FlyoutStyle::Pill).1);
    }
}
