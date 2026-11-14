//! What parts can do and react to: conditions ("only when the CPU is over
//! 90%"), click actions, and the names of the icons an icon part can show.
//!
//! A condition is one of:
//! - `stats.cpu > 90`: a live value compared with a number (`>`, `<`, `>=`, `<=`, `==`, `!=`).
//!   The number is read from the start of the value, so "12%" is 12 and "18°" is 18.
//! - `media.playing`: true while that value has something to say.
//! - `!media.playing`: the opposite.

/// What a click on a part does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open a program, a file, a folder or a web page.
    Open(String),
    /// Play or pause what is playing.
    ToggleMedia,
    /// Open the Lumen app.
    OpenLumen,
}

/// Reads a part's `click` setting: `open <target>`, `media.toggle` or
/// `lumen.open`. `None` for anything else, including nothing.
pub fn parse_action(text: &str) -> Option<Action> {
    let text = text.trim();
    match text {
        "media.toggle" => Some(Action::ToggleMedia),
        "lumen.open" => Some(Action::OpenLumen),
        _ => text.strip_prefix("open ").map(str::trim).filter(|target| !target.is_empty()).map(|target| Action::Open(target.to_owned())),
    }
}

/// Whether something is safe for a click to open: a web page (http or
/// https), or a path, which the caller must check exists. Never a command
/// line: there is nowhere to put arguments.
pub fn is_web_address(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && !target.chars().any(char::is_whitespace)
}

/// The number a live value starts with: "12%" → 12, "-3°" → -3, "14 km/h" → 14.
pub fn number_in(value: &str) -> Option<f64> {
    let value = value.trim();
    let end = value.char_indices().find(|(index, c)| !(c.is_ascii_digit() || *c == '.' || (*index == 0 && (*c == '-' || *c == '+')))).map_or(value.len(), |(index, _)| index);
    value[..end].parse().ok()
}

/// Whether a live value "has something to say": it is not empty, a dash, or a plain no.
fn says_something(value: &str) -> bool {
    !matches!(value.trim().to_lowercase().as_str(), "" | "–" | "-" | "n/a" | "no" | "false" | "0" | "0%")
}

/// Whether a condition holds, given how to look live values up. An empty
/// condition always holds. One that cannot be understood, or that compares a
/// value which is not a number right now, does not.
pub fn holds(condition: &str, value_of: &dyn Fn(&str) -> Option<String>) -> bool {
    let condition = condition.trim();
    if condition.is_empty() {
        return true;
    }
    for operator in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((left, right)) = condition.split_once(operator) {
            let (Some(value), Ok(wanted)) = (value_of(left.trim()).as_deref().and_then(number_in), right.trim().parse::<f64>()) else { return false };
            return match operator {
                ">=" => value >= wanted,
                "<=" => value <= wanted,
                "==" => (value - wanted).abs() < 1e-9,
                "!=" => (value - wanted).abs() >= 1e-9,
                ">" => value > wanted,
                _ => value < wanted,
            };
        }
    }
    match condition.strip_prefix('!') {
        Some(source) => !value_of(source.trim()).is_some_and(|value| says_something(&value)),
        None => value_of(condition).is_some_and(|value| says_something(&value)),
    }
}

/// What is wrong with a condition as written, if anything (for widget files).
pub fn check_condition(condition: &str) -> Result<(), String> {
    let condition = condition.trim();
    let name_ok = |name: &str| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    for operator in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((left, right)) = condition.split_once(operator) {
            return if !name_ok(left.trim()) {
                Err(format!("`{}` is not the name of a live value", left.trim()))
            } else if right.trim().parse::<f64>().is_err() {
                Err(format!("`{}` is not a number; a condition compares a live value with a number, like `stats.cpu > 90`", right.trim()))
            } else {
                Ok(())
            };
        }
    }
    if condition.is_empty() || name_ok(condition.strip_prefix('!').unwrap_or(condition).trim()) { Ok(()) } else { Err(format!("`{condition}` is not a condition; write one like `stats.cpu > 90` or `media.playing`")) }
}

/// The icons an icon part can show by name, drawn from Windows' own icon
/// font (Segoe Fluent Icons). Anything else typed as an icon is drawn as
/// written, so an emoji works too.
pub const ICONS: [(&str, char); 36] = [
    ("home", '\u{E80F}'),
    ("settings", '\u{E713}'),
    ("search", '\u{E721}'),
    ("mail", '\u{E715}'),
    ("calendar", '\u{E787}'),
    ("clock", '\u{E823}'),
    ("music", '\u{E8D6}'),
    ("play", '\u{E768}'),
    ("pause", '\u{E769}'),
    ("next", '\u{E893}'),
    ("previous", '\u{E892}'),
    ("volume", '\u{E767}'),
    ("mute", '\u{E74F}'),
    ("wifi", '\u{E701}'),
    ("bluetooth", '\u{E702}'),
    ("folder", '\u{E8B7}'),
    ("document", '\u{E8A5}'),
    ("globe", '\u{E774}'),
    ("heart", '\u{EB51}'),
    ("star", '\u{E734}'),
    ("pin", '\u{E718}'),
    ("camera", '\u{E722}'),
    ("picture", '\u{E8B9}'),
    ("video", '\u{E714}'),
    ("download", '\u{E896}'),
    ("upload", '\u{E898}'),
    ("power", '\u{E7E8}'),
    ("lock", '\u{E72E}'),
    ("person", '\u{E77B}'),
    ("phone", '\u{E717}'),
    ("message", '\u{E8BD}'),
    ("cloud", '\u{E753}'),
    ("sun", '\u{E706}'),
    ("game", '\u{E7FC}'),
    ("link", '\u{E71B}'),
    ("info", '\u{E946}'),
];

/// The glyph for an icon name, if it is one of [`ICONS`].
pub fn icon_glyph(name: &str) -> Option<char> {
    let name = name.trim().to_lowercase();
    ICONS.iter().find(|(known, _)| *known == name).map(|(_, glyph)| *glyph)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(name: &str) -> Option<String> {
        Some(
            match name {
                "stats.cpu" => "93%",
                "stats.gpu" => "n/a",
                "weather.temp" => "-3°",
                "weather.wind" => "14 km/h",
                "media.playing" => "yes",
                "media.title" => "",
                _ => return None,
            }
            .to_owned(),
        )
    }

    #[test]
    fn numbers_are_read_from_the_start_of_a_value() {
        assert_eq!((number_in("12%"), number_in("-3°"), number_in("14 km/h"), number_in("4.5")), (Some(12.0), Some(-3.0), Some(14.0), Some(4.5)));
        assert_eq!((number_in("n/a"), number_in(""), number_in("Clear")), (None, None, None));
    }

    #[test]
    fn conditions_compare_live_values_with_numbers() {
        let holds = |condition: &str| holds(condition, &values);
        assert!(holds("stats.cpu > 90") && holds("stats.cpu>=93") && !holds("stats.cpu < 90") && holds("stats.cpu != 50") && holds("stats.cpu == 93"));
        assert!(holds("weather.temp < 0") && holds("weather.wind <= 14"));
        assert!(!holds("stats.gpu > 1"), "a value that is not a number right now never passes");
        assert!(!holds("nothing.here > 1") && !holds("stats.cpu > lots"));
    }

    #[test]
    fn conditions_can_ask_whether_a_value_has_anything_to_say() {
        let holds = |condition: &str| holds(condition, &values);
        assert!(holds("media.playing") && !holds("!media.playing"));
        assert!(!holds("media.title") && holds("!media.title"), "empty text is nothing");
        assert!(!holds("stats.gpu") && !holds("nothing.here") && holds("!nothing.here"));
        assert!(holds("") && holds("   "), "no condition always holds");
    }

    #[test]
    fn badly_written_conditions_are_explained() {
        assert!(check_condition("stats.cpu > 90").is_ok() && check_condition("!media.playing").is_ok() && check_condition("").is_ok());
        assert!(check_condition("stats.cpu > lots").unwrap_err().contains("not a number"));
        assert!(check_condition("the cpu > 90").unwrap_err().contains("not the name of a live value"));
        assert!(check_condition("is it raining?").unwrap_err().contains("not a condition"));
    }

    #[test]
    fn click_actions_are_read_and_nothing_else_is() {
        assert_eq!(parse_action("open C:\\Windows\\notepad.exe"), Some(Action::Open("C:\\Windows\\notepad.exe".into())));
        assert_eq!(parse_action("  open   https://example.com  "), Some(Action::Open("https://example.com".into())));
        assert_eq!((parse_action("media.toggle"), parse_action("lumen.open")), (Some(Action::ToggleMedia), Some(Action::OpenLumen)));
        assert_eq!((parse_action(""), parse_action("open "), parse_action("run something"), parse_action("opennotepad")), (None, None, None, None));
        assert!(is_web_address("https://example.com/a?b=1") && is_web_address("HTTP://example.com"));
        assert!(!is_web_address("file:///C:/x") && !is_web_address("javascript:alert(1)") && !is_web_address("https://a b") && !is_web_address("notepad.exe"));
    }

    #[test]
    fn icons_are_found_by_name() {
        assert_eq!(icon_glyph(" Home "), Some('\u{E80F}'));
        assert_eq!(icon_glyph("🌙"), None, "anything else is drawn as written");
        let mut names: Vec<&str> = ICONS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ICONS.len(), "no name twice");
    }
}
