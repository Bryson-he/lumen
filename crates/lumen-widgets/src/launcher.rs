//! The launcher panel: ranking apps against what was typed, its size, and
//! how it is drawn. Colours and fonts come from the theme's [`Paint`].

use std::collections::HashMap;

use lumen_render::{Bitmap, Canvas, Rect, Weight};
use lumen_win::apps::App;

use crate::paint::Paint;

pub const WIDTH: f32 = 620.0;
/// The most results shown at once.
pub const MAX_RESULTS: usize = 7;
/// The longest search text kept.
pub const MAX_QUERY: usize = 64;
const SEARCH_ROW: f32 = 60.0;
const ROW: f32 = 46.0;
/// Space above and below the list of results.
const LIST_PAD: f32 = 8.0;
const SIDE: f32 = 22.0;
const ICON: f32 = 28.0;

/// The score of a match that is only the typed letters in order, with gaps.
const SCATTERED: u32 = 200;

/// How well `name` answers `query` (already lower-case); `None` if not at all.
fn score(name: &str, query: &str) -> Option<u32> {
    let name = name.to_lowercase();
    if name == query {
        return Some(1000);
    }
    if name.starts_with(query) {
        return Some(800);
    }
    // "code" finds "Visual Studio Code"; "vsc" finds it by its initials.
    if name.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(query)) {
        return Some(600);
    }
    let initials: String = name.split(|c: char| !c.is_alphanumeric()).filter_map(|word| word.chars().next()).collect();
    if initials.starts_with(query) {
        return Some(500);
    }
    if name.contains(query) {
        return Some(400);
    }
    // The letters in order, with gaps: "ntpd" finds "Notepad".
    let mut wanted = query.chars().filter(|c| !c.is_whitespace()).peekable();
    for c in name.chars() {
        if wanted.peek() == Some(&c) {
            wanted.next();
        }
    }
    wanted.peek().is_none().then_some(SCATTERED)
}

/// The apps to show for `query`, best first. With nothing typed: the apps
/// opened most from the launcher, then the rest by name.
pub fn search<'a>(apps: &'a [App], query: &str, usage: &HashMap<String, u32>, limit: usize) -> Vec<&'a App> {
    let query = query.trim().to_lowercase();
    let used = |app: &App| usage.get(&app.target).copied().unwrap_or(0);
    let mut ranked: Vec<(u32, &App)> = if query.is_empty() {
        apps.iter().map(|app| (used(app), app)).collect()
    } else {
        let mut scored: Vec<(u32, &App)> = apps.iter().filter_map(|app| score(&app.name, &query).map(|score| (score, app))).collect();
        // Scattered letters are a last resort: shown only when nothing matches properly.
        if scored.iter().any(|(score, _)| *score > SCATTERED) {
            scored.retain(|(score, _)| *score > SCATTERED);
        }
        // Habit breaks ties and nudges close calls, but never beats a clearly better match.
        scored.into_iter().map(|(score, app)| (score + used(app).min(40) * 4, app)).collect()
    };
    ranked.sort_by(|(a_score, a), (b_score, b)| b_score.cmp(a_score).then(a.name.len().cmp(&b.name.len())).then_with(|| a.name.cmp(&b.name)));
    if query.is_empty() {
        // Unused apps keep their alphabetical order rather than "shortest first".
        ranked.sort_by(|(a_score, a), (b_score, b)| b_score.cmp(a_score).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    }
    ranked.into_iter().take(limit).map(|(_, app)| app).collect()
}

/// The panel's height for a number of results.
pub fn height(results: usize) -> f32 {
    // With no results one row is kept for the "nothing found" line.
    SEARCH_ROW + LIST_PAD * 2.0 + ROW * results.max(1) as f32
}

/// The result under a point `y` logical pixels from the panel's top.
pub fn row_at(y: f32, results: usize) -> Option<usize> {
    let offset = y - SEARCH_ROW - LIST_PAD;
    (offset >= 0.0 && offset < ROW * results as f32).then(|| (offset / ROW) as usize)
}

/// What the panel shows right now.
pub struct View<'a> {
    pub query: &'a str,
    pub results: &'a [&'a App],
    /// Index of the highlighted result.
    pub selected: usize,
}

pub fn draw(canvas: &Canvas<'_>, paint: &Paint, view: &View<'_>, icons: &HashMap<String, Bitmap>, width: f32, height: f32) {
    let View { query, results, selected } = *view;
    // A panel over other windows: solid, so the text stays readable.
    let mut card = paint.clone();
    card.glass = paint.surface;
    card.card(canvas, width, height);

    // What was typed, with a caret after it.
    let text_y = (SEARCH_ROW - 26.0) / 2.0;
    let line = Rect::new(SIDE, text_y, width - SIDE * 2.0, 26.0);
    if query.is_empty() {
        canvas.text("Search apps", line, paint.display(19.0, paint.text_muted));
        canvas.fill_round_rect(Rect::new(SIDE - 4.0, text_y + 2.0, 2.0, 22.0), 1.0, paint.primary);
    } else {
        canvas.text(query, line, paint.display(19.0, paint.text));
        // Trailing spaces are not measured, so stand in a character for each.
        let measured = query.replace(' ', "\u{a0}");
        let caret_x = (SIDE + canvas.text_width(&measured, paint.display(19.0, paint.text)) + 2.0).min(width - SIDE);
        canvas.fill_round_rect(Rect::new(caret_x, text_y + 2.0, 2.0, 22.0), 1.0, paint.primary);
    }
    canvas.fill_round_rect(Rect::new(0.0, SEARCH_ROW - 1.0, width, 1.0), 0.0, paint.border);

    let top = SEARCH_ROW + LIST_PAD;
    if results.is_empty() {
        canvas.text("No apps match", Rect::new(SIDE, top + (ROW - 18.0) / 2.0, width - SIDE * 2.0, 18.0), paint.body(14.0, Weight::Regular, paint.text_muted));
        return;
    }
    for (index, app) in results.iter().enumerate() {
        let y = top + ROW * index as f32;
        if index == selected {
            let highlight = Rect::new(LIST_PAD, y + 2.0, width - LIST_PAD * 2.0, ROW - 4.0);
            canvas.fill_round_rect(highlight, paint.radius.min(10.0), paint.track);
            canvas.fill_round_rect(Rect::new(LIST_PAD, y + 12.0, 3.0, ROW - 24.0), 1.5, paint.primary);
        }
        let icon = Rect::new(SIDE, y + (ROW - ICON) / 2.0, ICON, ICON);
        match icons.get(&app.target) {
            Some(bitmap) => canvas.image(bitmap, icon, 0.0),
            None => {
                canvas.fill_round_rect(icon, 6.0, paint.track);
                let initial: String = app.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                canvas.text(&initial, Rect::new(icon.x + ICON / 2.0 - 5.0, icon.y + 5.0, 16.0, 18.0), paint.body(13.0, Weight::SemiBold, paint.text));
            }
        }
        let weight = if index == selected { Weight::Medium } else { Weight::Regular };
        canvas.text(&app.name, Rect::new(SIDE + ICON + 14.0, y + (ROW - 20.0) / 2.0, width - SIDE * 2.0 - ICON - 14.0, 20.0), paint.body(15.0, weight, paint.text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(names: &[&str]) -> Vec<App> {
        names.iter().map(|name| App { name: (*name).to_owned(), target: format!("id:{name}") }).collect()
    }

    fn names(found: Vec<&App>) -> Vec<&str> {
        found.into_iter().map(|app| app.name.as_str()).collect()
    }

    #[test]
    fn better_matches_come_first() {
        let list = apps(&["Visual Studio Code", "Codecs Pack", "Barcode Scanner", "Code", "Notepad"]);
        let found = names(search(&list, "code", &HashMap::new(), 7));
        assert_eq!(found, ["Code", "Codecs Pack", "Visual Studio Code", "Barcode Scanner"], "exact, prefix, word, inside");
    }

    #[test]
    fn initials_and_scattered_letters_still_find_things() {
        let list = apps(&["Visual Studio Code", "Notepad", "Paint"]);
        assert_eq!(names(search(&list, "vsc", &HashMap::new(), 7)), ["Visual Studio Code"]);
        assert_eq!(names(search(&list, "ntpd", &HashMap::new(), 7)), ["Notepad"]);
        assert!(search(&list, "xyz", &HashMap::new(), 7).is_empty());
        assert_eq!(names(search(&list, "  PAINT ", &HashMap::new(), 7)), ["Paint"], "case and spaces around do not matter");
    }

    #[test]
    fn habit_breaks_ties_but_not_clear_wins() {
        let list = apps(&["Steam", "Stream Deck", "Mainstream"]);
        let mut usage = HashMap::new();
        usage.insert("id:Stream Deck".to_owned(), 30);
        usage.insert("id:Mainstream".to_owned(), 30);
        assert_eq!(names(search(&list, "st", &usage, 7)), ["Stream Deck", "Steam", "Mainstream"], "two prefix matches: the used one first; an inside match stays last");
    }

    #[test]
    fn nothing_typed_shows_favourites_then_the_alphabet() {
        let list = apps(&["Zed", "alpha", "Beta", "Calculator"]);
        let mut usage = HashMap::new();
        usage.insert("id:Zed".to_owned(), 3);
        assert_eq!(names(search(&list, "", &usage, 3)), ["Zed", "alpha", "Beta"]);
    }

    #[test]
    fn clicks_map_to_rows() {
        assert_eq!(row_at(10.0, 3), None, "the search box");
        assert_eq!(row_at(SEARCH_ROW + LIST_PAD + 1.0, 3), Some(0));
        assert_eq!(row_at(SEARCH_ROW + LIST_PAD + ROW * 2.5, 3), Some(2));
        assert_eq!(row_at(SEARCH_ROW + LIST_PAD + ROW * 3.5, 3), None, "below the last row");
        assert!(height(0) == height(1) && height(7) > height(1));
    }
}
