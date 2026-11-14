//! Home Assistant: showing and switching the things it knows about.
//!
//! Once Lumen is given Home Assistant's address and an access token, every
//! entity is a live value any part can show, and a click can switch it:
//!
//! - `{ha.sensor.office_temperature}`: its state with its unit, "21.5 °C"
//! - `{ha.light.desk.name}`: its friendly name
//! - `ha.light.desk.on`: for conditions, true while it is on, open, home, playing…
//! - `{ha.light.desk.brightness}`: any attribute Home Assistant reports
//! - `click: ha.toggle light.desk`: switch it (or run it, for a scene, script or button)
//!
//! The token stays on this PC, in a file of its own. It is never part of a
//! widget file, a preset or anything shared: those only name entities.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// How Lumen reaches Home Assistant (`home-assistant.json`). Never sent to the settings app.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HaConfig {
    /// As typed into a browser: `http://homeassistant.local:8123`.
    pub address: String,
    /// A long-lived access token.
    pub token: String,
}

impl HaConfig {
    pub fn is_set(&self) -> bool {
        !self.address.is_empty() && !self.token.is_empty()
    }

    /// Tidies what was typed: no spaces around either, no slash on the end
    /// of the address, `http://` if no scheme was given, and no "Bearer " in
    /// front of the token (people paste it both ways).
    pub fn sanitised(self) -> Self {
        let mut address = self.address.trim().trim_end_matches('/').to_owned();
        if !address.is_empty() && !address.starts_with("http://") && !address.starts_with("https://") {
            address = format!("http://{address}");
        }
        let token = self.token.trim().trim_matches('"');
        let token = token.strip_prefix("Bearer ").or_else(|| token.strip_prefix("bearer ")).unwrap_or(token).trim();
        Self { address: address.chars().take(200).collect(), token: token.chars().filter(|c| !c.is_whitespace()).take(600).collect() }
    }
}

/// Where the connection stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaStatus {
    /// No address or token yet.
    #[default]
    NotSet,
    /// Set, and being tried.
    Checking,
    Connected,
    /// Home Assistant answered, and refused the token. Lumen stops asking until the token changes.
    Refused,
    /// Home Assistant could not be reached.
    Unreachable,
}

/// The connection as the settings app sees it: everything but the token.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaState {
    pub address: String,
    pub has_token: bool,
    pub status: HaStatus,
    /// What Home Assistant calls itself ("Home"), once connected.
    pub name: Option<String>,
    /// What went wrong, in plain words.
    pub error: Option<String>,
}

/// Params of `setHomeAssistant`. A token left out (or empty) keeps the one already saved.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SetHaParams {
    pub address: String,
    pub token: Option<String>,
}

/// One thing Home Assistant knows about, for the list to pick from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaEntity {
    /// `light.desk`
    pub id: String,
    pub name: String,
    /// Its state as shown: "on", "21.5 °C".
    pub state: String,
    /// Whether a click can switch or run it.
    pub clickable: bool,
}

/// What Lumen holds about one entity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HaValue {
    pub state: String,
    pub name: String,
    pub unit: String,
    /// Every attribute that is plain text, a number or true/false, as text.
    pub attributes: BTreeMap<String, String>,
}

/// States that count as "on" for conditions and the on/off dot.
const ON_STATES: [&str; 9] = ["on", "open", "opening", "unlocked", "home", "playing", "active", "heat", "cool"];

impl HaValue {
    /// Reads Home Assistant's JSON for one entity.
    pub fn from_json(entity: &serde_json::Value) -> Self {
        let text = |value: &serde_json::Value| match value {
            serde_json::Value::String(text) => Some(text.chars().take(200).collect::<String>()),
            serde_json::Value::Number(number) => Some(number.to_string()),
            serde_json::Value::Bool(flag) => Some(if *flag { "yes".to_owned() } else { String::new() }),
            _ => None,
        };
        let attributes: BTreeMap<String, String> = entity.get("attributes").and_then(serde_json::Value::as_object).into_iter().flatten().filter_map(|(name, value)| Some((name.clone(), text(value)?))).take(64).collect();
        Self {
            state: entity.get("state").and_then(text).unwrap_or_default(),
            name: attributes.get("friendly_name").cloned().unwrap_or_else(|| entity.get("entity_id").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned()),
            unit: attributes.get("unit_of_measurement").cloned().unwrap_or_default(),
            attributes,
        }
    }

    pub fn is_on(&self) -> bool {
        ON_STATES.contains(&self.state.as_str())
    }

    /// The state as a person would read it: "21.5 °C", "On", "Unavailable".
    pub fn shown(&self) -> String {
        if !self.unit.is_empty() && self.state.parse::<f64>().is_ok() {
            // A percentage is written without a space, so bars and rings can read it.
            return if self.unit == "%" { format!("{}%", self.state) } else { format!("{} {}", self.state, self.unit) };
        }
        let mut words = self.state.replace('_', " ");
        if let Some(first) = words.get(..1).map(str::to_uppercase) {
            words.replace_range(..1, &first);
        }
        words
    }

    /// One of this entity's values by name: nothing for the state as shown,
    /// `state`, `name`, `unit`, `on`, or any attribute.
    pub fn field(&self, field: &str) -> Option<String> {
        Some(match field {
            "" => self.shown(),
            "state" => self.state.clone(),
            "name" => self.name.clone(),
            "unit" => self.unit.clone(),
            "on" => if self.is_on() { "yes".to_owned() } else { String::new() },
            attribute => self.attributes.get(attribute)?.clone(),
        })
    }
}

/// Whether text is an entity id: `domain.object`, lower-case letters, digits and underscores.
pub fn is_entity_id(text: &str) -> bool {
    let ok = |piece: &str| !piece.is_empty() && piece.len() <= 80 && piece.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    text.split_once('.').is_some_and(|(domain, object)| ok(domain) && ok(object))
}

/// Splits a live value's name into the entity and the field asked for:
/// `ha.light.desk.name` → (`light.desk`, `name`). `None` if it is not a Home Assistant value.
pub fn split_source(source: &str) -> Option<(String, &str)> {
    let rest = source.strip_prefix("ha.")?;
    let mut pieces = rest.splitn(3, '.');
    let (domain, object) = (pieces.next()?, pieces.next()?);
    let entity = format!("{domain}.{object}");
    is_entity_id(&entity).then(|| (entity, pieces.next().unwrap_or("")))
}

/// Every entity mentioned anywhere in a piece of text (a part's text, source, condition or click).
pub fn entities_in(text: &str, found: &mut BTreeSet<String>) {
    let name_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.';
    let mut from = 0;
    while let Some(offset) = text[from..].find("ha.") {
        let at = from + offset;
        from = at + 3;
        // Only at the start of a word: "alpha.x.y" is not one.
        if text[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let rest = &text[at..];
        // A click names its entity after a space: "ha.toggle light.desk".
        if let Some(named) = rest.strip_prefix("ha.toggle ") {
            let id: String = named.trim_start().chars().take_while(|c| name_char(*c)).collect();
            if is_entity_id(&id) {
                found.insert(id);
            }
            continue;
        }
        let name: String = rest.chars().take_while(|c| name_char(*c)).collect();
        if let Some((entity, _)) = split_source(&name) {
            found.insert(entity);
        }
    }
}

/// Whether an entity is only ever run (a scene, a script, a button): it has no state worth showing.
pub fn is_run_only(entity: &str) -> bool {
    matches!(entity.split('.').next().unwrap_or_default(), "scene" | "script" | "button" | "input_button" | "automation")
}

/// Whether a click can do something to an entity of this kind.
pub fn is_clickable(entity: &str) -> bool {
    let domain = entity.split('.').next().unwrap_or_default();
    matches!(domain, "light" | "switch" | "fan" | "input_boolean" | "cover" | "lock" | "scene" | "script" | "button" | "input_button" | "media_player" | "automation" | "siren" | "humidifier" | "vacuum" | "group" | "climate" | "remote" | "valve")
}

/// The Home Assistant service a click calls for an entity, as `(domain, service)`.
/// Most things toggle; things that only run are run; a lock goes the other way from where it is.
pub fn click_service(entity: &str, state: &str) -> (&'static str, &'static str) {
    match entity.split('.').next().unwrap_or_default() {
        "scene" => ("scene", "turn_on"),
        "script" => ("script", "turn_on"),
        "button" => ("button", "press"),
        "input_button" => ("input_button", "press"),
        "automation" => ("automation", "trigger"),
        "lock" => ("lock", if state == "locked" { "unlock" } else { "lock" }),
        "media_player" => ("media_player", "media_play_pause"),
        "vacuum" => ("vacuum", if state == "cleaning" { "return_to_base" } else { "start" }),
        _ => ("homeassistant", "toggle"),
    }
}

/// How tall one entity's row is in a panel, and how far in the rows sit.
pub const ROW_HEIGHT: f32 = 32.0;
const ROW_MARGIN: f32 = 16.0;
/// The most entities one panel (or one "add to this widget") takes.
pub const MAX_ROWS: usize = 12;

/// The parts that show a list of entities as rows, one under another: a dot
/// and a name on the left, the state on the right, and a click that switches
/// (or runs) the ones that can be. `top` is where the first row starts and
/// `width` how wide the widget is. Ids are made from the entity ids, so two
/// rows for different entities never clash.
pub fn rows(entities: &[(String, String)], top: f32, width: f32) -> Vec<crate::parts::Part> {
    use crate::parts::{Part, PartAlign, PartColor, PartKind, PartWeight};
    let inner = (width - ROW_MARGIN * 2.0).max(120.0);
    let mut parts = Vec::new();
    for (index, (entity, name)) in entities.iter().filter(|(entity, _)| is_entity_id(entity)).take(MAX_ROWS).enumerate() {
        let y = top + ROW_HEIGHT * index as f32;
        // "light.desk_lamp" -> "light-desk-lamp", short enough to leave room for a suffix.
        let slug: String = entity.replace(['.', '_'], "-").chars().take(38).collect();
        let click = if is_clickable(entity) { format!("ha.toggle {entity}") } else { String::new() };
        let label = if name.trim().is_empty() { entity.clone() } else { name.trim().chars().take(40).collect() };
        let row = |suffix: &str, title: &str| Part { id: format!("{slug}-{suffix}"), name: format!("{label}: {title}"), click: click.clone(), ..Default::default() };
        let text_x = ROW_MARGIN + 22.0;
        if is_run_only(entity) {
            parts.push(Part { kind: PartKind::Icon, text: "play".into(), color: PartColor::Highlight, x: ROW_MARGIN - 2.0, y: y + 5.0, width: 18.0, height: 18.0, ..row("icon", "run") });
            parts.push(Part { text: format!("{{ha.{entity}.name}}"), size: 14.0, x: text_x, y: y + 4.0, width: inner - 22.0, height: 22.0, stretch: true, ..row("name", "name") });
            continue;
        }
        if is_clickable(entity) {
            parts.push(Part { kind: PartKind::Shape, color: PartColor::Highlight, radius: 5.0, when: format!("ha.{entity}.on"), x: ROW_MARGIN, y: y + 10.0, width: 10.0, height: 10.0, ..row("on", "on") });
            parts.push(Part { kind: PartKind::Shape, color: PartColor::Outline, radius: 5.0, when: format!("!ha.{entity}.on"), x: ROW_MARGIN, y: y + 10.0, width: 10.0, height: 10.0, ..row("off", "off") });
        }
        let name_x = if is_clickable(entity) { text_x } else { ROW_MARGIN };
        let state_width = (inner * 0.4).round();
        parts.push(Part { text: format!("{{ha.{entity}.name}}"), size: 14.0, x: name_x, y: y + 4.0, width: inner - state_width - (name_x - ROW_MARGIN) - 8.0, height: 22.0, stretch: true, ..row("name", "name") });
        parts.push(Part {
            text: format!("{{ha.{entity}}}"),
            size: 14.0,
            weight: PartWeight::Medium,
            color: if is_clickable(entity) { PartColor::Muted } else { PartColor::Highlight },
            align: PartAlign::Right,
            x: ROW_MARGIN + inner - state_width,
            y: y + 4.0,
            width: state_width,
            height: 22.0,
            ..row("state", "state")
        });
    }
    parts
}

/// A widget file showing several entities as rows, under a title.
pub fn panel_yaml(entities: &[(String, String)], title: &str) -> String {
    use crate::parts::{Part, PartWeight};
    let title: String = title.trim().chars().filter(|c| !c.is_control()).take(60).collect();
    let title = if title.is_empty() { "Home".to_owned() } else { title };
    let width = 280.0;
    let count = entities.iter().filter(|(entity, _)| is_entity_id(entity)).take(MAX_ROWS).count();
    let mut parts = vec![Part { id: "title".into(), name: "Title".into(), text: title.clone(), size: 15.0, weight: PartWeight::SemiBold, x: ROW_MARGIN, y: 12.0, width: width - ROW_MARGIN * 2.0, height: 22.0, stretch: true, ..Default::default() }];
    parts.extend(rows(entities, 42.0, width));
    // The right-hand states keep to the right edge when the widget is made wider.
    let size = (width, 42.0 + ROW_HEIGHT * count as f32 + 10.0);
    crate::widget_file::render(&title, size, &parts, None, &|_| None)
}

/// A widget file showing one entity: a switch for things a click can change, a reading for the rest.
pub fn widget_yaml(entity: &str, name: &str) -> String {
    let title = name.replace(['"', '\\', '\n'], " ");
    if is_run_only(entity) {
        return format!(
            "# {title}, from Home Assistant. Click it to run it.\nlumen-widget: 1\nname: \"{title}\"\nsize: [200, 64]\n\nparts:\n  - id: icon\n    kind: icon\n    text: play\n    color: highlight\n    click: ha.toggle {entity}\n    at: [14, 20]\n    box: [24, 24]\n  - id: name\n    text: \"{{ha.{entity}.name}}\"\n    weight: semibold\n    size: 15\n    click: ha.toggle {entity}\n    at: [48, 12]\n    box: [136, 22]\n    stretch: true\n  - id: hint\n    text: Click to run\n    color: muted\n    size: 12\n    click: ha.toggle {entity}\n    at: [48, 34]\n    box: [136, 18]\n    stretch: true\n"
        );
    }
    if is_clickable(entity) {
        format!(
            "# {title}, from Home Assistant. Click it to switch it.\nlumen-widget: 1\nname: \"{title}\"\nsize: [220, 72]\n\nparts:\n  - id: lit\n    kind: shape\n    color: highlight\n    radius: 6\n    when: ha.{entity}.on\n    at: [16, 20]\n    box: [12, 12]\n  - id: unlit\n    kind: shape\n    color: outline\n    radius: 6\n    when: \"!ha.{entity}.on\"\n    at: [16, 20]\n    box: [12, 12]\n  - id: name\n    text: \"{{ha.{entity}.name}}\"\n    weight: semibold\n    size: 15\n    click: ha.toggle {entity}\n    at: [38, 13]\n    box: [166, 22]\n    stretch: true\n  - id: state\n    text: \"{{ha.{entity}}}\"\n    color: muted\n    size: 13\n    click: ha.toggle {entity}\n    at: [38, 38]\n    box: [166, 20]\n    stretch: true\n"
        )
    } else {
        format!(
            "# {title}, from Home Assistant.\nlumen-widget: 1\nname: \"{title}\"\nsize: [200, 84]\n\nparts:\n  - id: name\n    text: \"{{ha.{entity}.name}}\"\n    color: muted\n    size: 13\n    at: [16, 12]\n    box: [168, 20]\n    stretch: true\n  - id: value\n    text: \"{{ha.{entity}}}\"\n    font: Space Grotesk\n    weight: semibold\n    color: highlight\n    size: 28\n    at: [16, 34]\n    box: [168, 38]\n    stretch: true\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<String> {
        let mut set = BTreeSet::new();
        entities_in(text, &mut set);
        set.into_iter().collect()
    }

    #[test]
    fn what_was_typed_is_tidied() {
        let typed = HaConfig { address: " homeassistant.local:8123/ ".into(), token: "\"Bearer abc.def-ghi \n\"".into() }.sanitised();
        assert_eq!((typed.address.as_str(), typed.token.as_str()), ("http://homeassistant.local:8123", "abc.def-ghi"));
        assert_eq!(HaConfig { address: "https://ha.example.com".into(), token: "t".into() }.sanitised().address, "https://ha.example.com");
        assert!(!HaConfig::default().sanitised().is_set() && typed.is_set());
    }

    #[test]
    fn an_entity_is_read_and_its_values_are_found_by_name() {
        let light = HaValue::from_json(&serde_json::json!({ "entity_id": "light.desk", "state": "on", "attributes": { "friendly_name": "Desk lamp", "brightness": 128, "rgb": [1, 2, 3], "locked": false } }));
        assert_eq!((light.field("").unwrap().as_str(), light.field("name").unwrap().as_str(), light.field("on").unwrap().as_str(), light.field("brightness").unwrap().as_str()), ("On", "Desk lamp", "yes", "128"));
        assert_eq!((light.field("rgb"), light.field("locked"), light.field("nonsense")), (None, Some(String::new()), None));
        let temperature = HaValue::from_json(&serde_json::json!({ "entity_id": "sensor.office", "state": "21.5", "attributes": { "unit_of_measurement": "°C" } }));
        assert_eq!((temperature.shown().as_str(), temperature.field("state").unwrap().as_str(), temperature.field("on").unwrap().as_str(), temperature.name.as_str()), ("21.5 °C", "21.5", "", "sensor.office"));
        let battery = HaValue::from_json(&serde_json::json!({ "state": "83", "attributes": { "unit_of_measurement": "%" } }));
        assert_eq!(battery.shown(), "83%", "so a bar or ring can show it");
        let gone = HaValue::from_json(&serde_json::json!({ "state": "not_home" }));
        assert_eq!(gone.shown(), "Not home");
    }

    #[test]
    fn names_of_live_values_are_taken_apart() {
        assert_eq!(split_source("ha.light.desk"), Some(("light.desk".into(), "")));
        assert_eq!(split_source("ha.switch.neon_light.name"), Some(("switch.neon_light".into(), "name")));
        assert_eq!(split_source("ha.sensor.x.attr.with.dots"), Some(("sensor.x".into(), "attr.with.dots")));
        for not in ["ha.light", "ha.", "stats.cpu", "ha.Light.desk", "has.light.desk"] {
            assert_eq!(split_source(not), None, "{not}");
        }
    }

    #[test]
    fn entities_are_found_wherever_a_part_mentions_them() {
        assert_eq!(found("{ha.light.desk.name} is {ha.light.desk} and {ha.sensor.temp_1} outside"), ["light.desk", "sensor.temp_1"]);
        assert_eq!(found("ha.switch.neon_light.on"), ["switch.neon_light"]);
        assert_eq!(found("!ha.lock.front_door.on"), ["lock.front_door"]);
        assert_eq!(found("ha.toggle scene.movie_night"), ["scene.movie_night"]);
        assert_eq!(found("ha.cover.garage.current_position > 50"), ["cover.garage"]);
        assert!(found("alpha.light.desk, {stats.cpu}, ha.light, plain text").is_empty());
    }

    #[test]
    fn a_click_does_the_sensible_thing_for_each_kind() {
        assert_eq!(click_service("light.desk", "on"), ("homeassistant", "toggle"));
        assert_eq!(click_service("scene.movie", "scening"), ("scene", "turn_on"));
        assert_eq!(click_service("button.restart", "unknown"), ("button", "press"));
        assert_eq!((click_service("lock.door", "locked"), click_service("lock.door", "unlocked")), (("lock", "unlock"), ("lock", "lock")));
        assert!(is_clickable("switch.neon_light") && is_clickable("script.goodnight") && !is_clickable("sensor.temperature") && !is_clickable("weather.home"));
    }

    #[test]
    fn a_made_widget_reads_as_a_widget_file() {
        for (entity, name) in [("switch.neon_light", "Neon \"light\""), ("sensor.office_temperature", "Office"), ("scene.movie_night", "Movie night")] {
            if is_run_only(entity) {
                let file = crate::widget_file::parse(&widget_yaml(entity, name), &|_| None).unwrap();
                assert!(file.parts.iter().all(|part| part.click == format!("ha.toggle {entity}")) && file.parts.iter().any(|part| part.text == "Click to run"));
                continue;
            }
            let text = widget_yaml(entity, name);
            let file = crate::widget_file::parse(&text, &|_| None).unwrap_or_else(|error| panic!("{entity}: {} in\n{text}", error.message));
            assert!(file.parts.iter().any(|part| part.text.contains(&format!("{{ha.{entity}}}"))));
            assert_eq!(file.parts.iter().any(|part| part.click == format!("ha.toggle {entity}")), is_clickable(entity));
        }
    }

    #[test]
    fn several_entities_make_one_panel_with_a_row_each() {
        let chosen = vec![("switch.neon_light".to_owned(), "Neon light".to_owned()), ("sensor.office_temperature".to_owned(), "Office".to_owned()), ("scene.movie_night".to_owned(), "Movie night".to_owned()), ("not an entity".to_owned(), "Junk".to_owned())];
        let text = panel_yaml(&chosen, "Living room");
        let file = crate::widget_file::parse(&text, &|_| None).unwrap_or_else(|error| panic!("{} in\n{text}", error.message));
        assert_eq!((file.name.as_str(), file.size), ("Living room", (280.0, 42.0 + 3.0 * ROW_HEIGHT + 10.0)));
        let ids: Vec<&str> = file.parts.iter().map(|part| part.id.as_str()).collect();
        assert_eq!(ids, ["title", "switch-neon-light-on", "switch-neon-light-off", "switch-neon-light-name", "switch-neon-light-state", "sensor-office-temperature-name", "sensor-office-temperature-state", "scene-movie-night-icon", "scene-movie-night-name"]);
        let part = |id: &str| file.parts.iter().find(|part| part.id == id).unwrap();
        // A switch is clicked anywhere on its row; a reading is not clickable; a scene is run.
        assert_eq!((part("switch-neon-light-name").click.as_str(), part("sensor-office-temperature-state").click.as_str(), part("scene-movie-night-name").click.as_str()), ("ha.toggle switch.neon_light", "", "ha.toggle scene.movie_night"));
        assert_eq!((part("switch-neon-light-on").when.as_str(), part("switch-neon-light-off").when.as_str()), ("ha.switch.neon_light.on", "!ha.switch.neon_light.on"));
        // Rows sit one under another and nothing overlaps the next row.
        assert_eq!((part("switch-neon-light-name").y, part("sensor-office-temperature-name").y, part("scene-movie-night-name").y), (46.0, 78.0, 110.0));
        // Every entity mentioned is found again, so Lumen knows what to ask Home Assistant for.
        let mut found = BTreeSet::new();
        for part in &file.parts {
            for text in [&part.text, &part.when, &part.click] {
                entities_in(text, &mut found);
            }
        }
        assert_eq!(found.into_iter().collect::<Vec<_>>(), ["scene.movie_night", "sensor.office_temperature", "switch.neon_light"]);

        // Added to a widget that already has parts: the rows start where asked, and at most twelve are taken.
        let many: Vec<(String, String)> = (0..20).map(|n| (format!("light.lamp_{n}"), format!("Lamp {n}"))).collect();
        let added = rows(&many, 200.0, 320.0);
        assert_eq!(added.len(), MAX_ROWS * 4);
        assert!(added.iter().all(|part| part.y >= 200.0 && part.id.len() <= 48));
        assert_eq!(crate::parts::sanitise_parts(added.clone()).len(), added.len(), "ids are plain and there is room for them all");
    }

}
