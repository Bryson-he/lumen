//! Where a widget lands when it is dragged: snapped to other widgets' edges
//! first, then to the grid, and always kept on its monitor.
//!
//! One implementation serves both the real desktop and the layout preview in
//! the settings app, so they can never disagree.

use lumen_ipc::widgets::{GRID, MIN_SIZE};

/// How close (logical pixels) an edge must be to another widget's edge to snap to it.
const EDGE_REACH: f32 = 8.0;

/// How close a widget's middle must be to the monitor's middle to snap to it.
const CENTRE_REACH: f32 = 12.0;

/// A rectangle in logical pixels, relative to a monitor's work area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The result of a move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapped {
    pub x: f32,
    pub y: f32,
    /// Where to draw a vertical guide (an x position), if an edge was matched.
    pub guide_x: Option<f32>,
    /// Where to draw a horizontal guide (a y position).
    pub guide_y: Option<f32>,
}

/// Snaps a widget being moved to `wanted`. `others` are the other widgets on
/// the same monitor; `area` is that monitor's work area size.
pub fn snap_move(wanted: Frame, others: &[Frame], area: (f32, f32), snap: bool) -> Snapped {
    let (mut x, mut guide_x) = (wanted.x, None);
    let (mut y, mut guide_y) = (wanted.y, None);
    if snap {
        (x, guide_x) = snap_axis(wanted.x, wanted.width, area.0, others.iter().map(|o| (o.x, o.width)));
        (y, guide_y) = snap_axis(wanted.y, wanted.height, area.1, others.iter().map(|o| (o.y, o.height)));
    }

    // Never off the monitor. If that moves it off a matched edge, drop the guide.
    let clamped_x = x.clamp(0.0, (area.0 - wanted.width).max(0.0));
    let clamped_y = y.clamp(0.0, (area.1 - wanted.height).max(0.0));
    Snapped {
        x: clamped_x,
        y: clamped_y,
        guide_x: guide_x.filter(|_| clamped_x == x),
        guide_y: guide_y.filter(|_| clamped_y == y),
    }
}

/// Snaps a size being dragged: whole grid steps when snapping, never below the minimum.
pub fn snap_size(width: f32, height: f32, snap: bool) -> (f32, f32) {
    let round = |value: f32| if snap { (value / GRID).round() * GRID } else { value.round() };
    (round(width).max(MIN_SIZE.0), round(height).max(MIN_SIZE.1))
}

/// One axis: returns the snapped start position and, if an edge of another
/// widget was matched, the coordinate of that shared line.
fn snap_axis(start: f32, size: f32, area: f32, others: impl Iterator<Item = (f32, f32)>) -> (f32, Option<f32>) {
    let mut best: Option<(f32, f32, f32)> = None; // (distance, position, guide line)
    // The middle of the monitor: a widget centred on it, with the guide down the middle.
    let centred = (area - size) / 2.0;
    if area > 0.0 && (centred - start).abs() <= CENTRE_REACH {
        best = Some(((centred - start).abs(), centred, area / 2.0));
    }
    for (other_start, other_size) in others {
        let other_end = other_start + other_size;
        // Line up starts, line up ends, or sit flush against either side.
        let candidates = [(other_start, other_start), (other_end - size, other_end), (other_end, other_end), (other_start - size, other_start)];
        for (position, line) in candidates {
            let distance = (position - start).abs();
            if distance <= EDGE_REACH && best.is_none_or(|(closest, ..)| distance < closest) {
                best = Some((distance, position, line));
            }
        }
    }
    match best {
        Some((_, position, line)) => (position, Some(line)),
        None => ((start / GRID).round() * GRID, None),
    }
}

/// The first spot, scanning down the left edge then across, where a new
/// widget of this size does not overlap an existing one.
pub fn free_spot(size: (f32, f32), others: &[Frame], area: (f32, f32)) -> (f32, f32) {
    let margin = GRID * 2.0;
    let mut x = margin;
    while x + size.0 <= area.0 - GRID {
        let mut y = margin;
        while y + size.1 <= area.1 - GRID {
            let candidate = Frame { x, y, width: size.0, height: size.1 };
            if !others.iter().any(|other| overlaps(candidate, *other)) {
                return (x, y);
            }
            y += GRID;
        }
        x += GRID;
    }
    (margin, margin)
}

fn overlaps(a: Frame, b: Frame) -> bool {
    // A grid step of breathing room counts as part of each widget.
    a.x < b.x + b.width + GRID && b.x < a.x + a.width + GRID && a.y < b.y + b.height + GRID && b.y < a.y + a.height + GRID
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: (f32, f32) = (1920.0, 1040.0);

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame { x, y, width, height }
    }

    #[test]
    fn with_nothing_nearby_it_snaps_to_the_grid() {
        let snapped = snap_move(frame(130.0, 61.0, 190.0, 92.0), &[], AREA, true);
        assert_eq!((snapped.x, snapped.y), (120.0, 72.0));
        assert_eq!((snapped.guide_x, snapped.guide_y), (None, None));
    }

    #[test]
    fn snapping_off_leaves_the_position_alone() {
        let snapped = snap_move(frame(131.0, 61.0, 190.0, 92.0), &[frame(133.0, 300.0, 190.0, 92.0)], AREA, false);
        assert_eq!((snapped.x, snapped.y), (131.0, 61.0));
        assert_eq!(snapped.guide_x, None);
    }

    #[test]
    fn a_nearby_left_edge_wins_over_the_grid_and_shows_a_guide() {
        // The other widget sits at x = 50, which is not on the 24 px grid.
        let other = frame(50.0, 300.0, 190.0, 92.0);
        let snapped = snap_move(frame(55.0, 100.0, 320.0, 96.0), &[other], AREA, true);
        assert_eq!(snapped.x, 50.0);
        assert_eq!(snapped.guide_x, Some(50.0));
        assert_eq!(snapped.y, 96.0, "the other axis still uses the grid");
        assert_eq!(snapped.guide_y, None);
    }

    #[test]
    fn right_edges_line_up_too() {
        let other = frame(100.0, 300.0, 200.0, 92.0); // right edge at 300
        let snapped = snap_move(frame(176.0, 100.0, 120.0, 96.0), &[other], AREA, true);
        assert_eq!(snapped.x, 180.0, "its right edge (x + 120) meets 300");
        assert_eq!(snapped.guide_x, Some(300.0));
    }

    #[test]
    fn widgets_sit_flush_against_each_other() {
        let other = frame(100.0, 100.0, 190.0, 92.0); // bottom edge at 192
        let snapped = snap_move(frame(100.0, 197.0, 190.0, 150.0), &[other], AREA, true);
        assert_eq!(snapped.y, 192.0);
        assert_eq!(snapped.guide_y, Some(192.0));
    }

    #[test]
    fn the_closest_edge_is_chosen() {
        let near = frame(203.0, 400.0, 100.0, 60.0);
        let nearer = frame(198.0, 600.0, 100.0, 60.0);
        let snapped = snap_move(frame(200.0, 100.0, 100.0, 60.0), &[near, nearer], AREA, true);
        assert_eq!(snapped.x, 198.0);
    }

    #[test]
    fn it_never_leaves_the_monitor() {
        let top_left = snap_move(frame(-80.0, -20.0, 190.0, 92.0), &[], AREA, true);
        assert_eq!((top_left.x, top_left.y), (0.0, 0.0));
        let bottom_right = snap_move(frame(5000.0, 5000.0, 190.0, 92.0), &[], AREA, true);
        assert_eq!((bottom_right.x, bottom_right.y), (AREA.0 - 190.0, AREA.1 - 92.0));
    }

    #[test]
    fn a_guide_is_dropped_when_the_edge_would_push_it_off_screen() {
        // Flush against the left of a widget at x = 100 would mean x = -90.
        let other = frame(100.0, 300.0, 190.0, 92.0);
        let snapped = snap_move(frame(-95.0, 100.0, 190.0, 92.0), &[other], AREA, true);
        assert_eq!(snapped.x, 0.0);
        assert_eq!(snapped.guide_x, None);
    }

    #[test]
    fn sizes_round_to_the_grid_and_respect_the_minimum() {
        assert_eq!(snap_size(203.0, 110.0, true), (192.0, 120.0));
        assert_eq!(snap_size(203.4, 110.6, false), (203.0, 111.0));
        assert_eq!(snap_size(10.0, 10.0, true), MIN_SIZE);
    }

    #[test]
    fn new_widgets_get_a_free_spot() {
        assert_eq!(free_spot((190.0, 92.0), &[], AREA), (48.0, 48.0));
        let taken = [frame(48.0, 48.0, 190.0, 92.0)];
        let (x, y) = free_spot((190.0, 92.0), &taken, AREA);
        assert_eq!(x, 48.0);
        assert!(y >= 48.0 + 92.0 + GRID, "below the first, with a gap: {y}");
        // A full column pushes it across.
        let column = [frame(48.0, 0.0, 190.0, 1040.0)];
        assert!(free_spot((190.0, 92.0), &column, AREA).0 >= 48.0 + 190.0 + GRID);
    }

    #[test]
    fn a_widget_snaps_to_the_middle_of_the_monitor() {
        let area = (1920.0, 1040.0);
        let near = snap_move(Frame { x: 853.0, y: 300.0, width: 200.0, height: 100.0 }, &[], area, true);
        assert_eq!((near.x, near.guide_x), (860.0, Some(960.0)), "centred left to right, with a guide down the middle");
        assert_eq!(near.guide_y, None);
        let both = snap_move(Frame { x: 868.0, y: 462.0, width: 200.0, height: 100.0 }, &[], area, true);
        assert_eq!((both.x, both.y, both.guide_x, both.guide_y), (860.0, 470.0, Some(960.0), Some(520.0)));
        let far = snap_move(Frame { x: 800.0, y: 300.0, width: 200.0, height: 100.0 }, &[], area, true);
        assert_eq!(far.guide_x, None);
        let off = snap_move(Frame { x: 853.0, y: 300.0, width: 200.0, height: 100.0 }, &[], area, false);
        assert_eq!(off.x, 853.0, "not when snapping is off");
    }

}
