//! Widget files: a widget written as a readable YAML file that can be
//! edited by hand or by the designer.
//!
//! ```yaml
//! lumen-widget: 1
//! name: Big clock
//! size: [260, 120]
//! include: shared.yaml          # variables and styles a set of widgets shares
//! variables:
//!   big: 56                     # name a value once, use it as $big
//! styles:
//!   caption: { font: Inter, size: 13, color: muted }
//! parts:
//!   - id: time
//!     text: "{clock.time}"
//!     size: $big
//!     color: accent             # a theme role, never a hex colour
//!     at: [20, 28]
//!     box: [220, 64]
//!   - id: date
//!     text: "{clock.date}"
//!     style: caption
//!     at: [20, 14]
//!     box: [220, 18]
//! ```
//!
//! A widget can also name a script (see `docs/SCRIPTS.md`):
//!
//! ```yaml
//! script: stocks.luau           # a Luau file in the same folder
//! permissions: [network]        # what it asks to do: files, network, run
//! every: 60                     # seconds between runs (5 if not said)
//! ```
//!
//! [`parse`] turns a file into plain [`Part`]s; [`render`] writes parts back,
//! keeping the author's variables and styles wherever a value has not changed.

use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

use crate::{
    parts::{Part, PartAlign, PartColor, PartKind, PartWeight, sanitise_parts},
    widgets::MIN_SIZE,
};

/// The size a widget file gets if it does not say.
pub const DEFAULT_SIZE: (f32, f32) = (220.0, 110.0);
const PART_KEYS: [&str; 23] = [
    "id", "name", "kind", "text", "source", "style", "font", "size", "weight", "color", "opacity", "align", "at", "box", "stretch", "from-bottom", "radius", "hidden", "click", "when", "alert-when", "alert-color", "hover-only",
];

/// What is wrong with a file, in words for the person who wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    pub message: String,
    /// The line to look at (1-based), when it is known.
    pub line: Option<usize>,
}

impl FileError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), line: None }
    }
}

/// What a widget's script is allowed to do, once the user has agreed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Permissions {
    /// Read and write files, and look in folders.
    pub files: bool,
    /// Fetch from the internet.
    pub network: bool,
    /// Run programs, and open programs, files and web pages.
    pub run: bool,
}

/// The kinds of option a widget file can expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionKind {
    Number,
    Toggle,
    Text,
    Choice,
    /// A theme colour, by name.
    Color,
}

/// Something about a widget its author lets the person using it adjust.
/// In the file it is used like a variable: `size: $big`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetOption {
    pub name: String,
    pub label: String,
    pub kind: OptionKind,
    /// What it is until the user changes it.
    pub default: serde_json::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// For a choice: what can be picked.
    pub choices: Vec<String>,
}

/// The answers one widget on the desktop has given to its file's options.
pub type OptionValues = serde_json::Map<String, serde_json::Value>;

impl Permissions {
    /// What is asked for, in the words a widget file uses.
    pub fn names(self) -> Vec<&'static str> {
        [(self.files, "files"), (self.network, "network"), (self.run, "run")].into_iter().filter_map(|(asked, name)| asked.then_some(name)).collect()
    }
}

/// How often a script runs if its file does not say, and the limits on what it can say (seconds).
pub const DEFAULT_EVERY: u32 = 5;
pub const EVERY_RANGE: (u32, u32) = (1, 3600);
/// The largest script Lumen will read.
pub const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

/// Whether `name` is a plain script file name in the widgets folder: `stocks.luau`.
pub fn is_script_name(name: &str) -> bool {
    let stem = name.strip_suffix(".luau").unwrap_or("");
    !stem.is_empty() && stem.len() <= 64 && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The most options a file may expose.
pub const MAX_OPTIONS: usize = 24;

/// A widget file, read and worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetFile {
    /// What the file lets its user adjust.
    pub options: Vec<WidgetOption>,
    pub name: String,
    /// The size the parts are laid out for, and the size a new widget from this file starts at.
    pub size: (f32, f32),
    pub parts: Vec<Part>,
    /// The script that feeds this widget, if it has one: a `.luau` file in the same folder.
    pub script: Option<String>,
    /// What the script asks to be allowed to do.
    pub permissions: Permissions,
    /// Seconds between the script's runs.
    pub every: u32,
}

/// One file in the widgets folder, as the settings app lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFileInfo {
    /// The file's name in the widgets folder: `big-clock.yaml`.
    pub file: String,
    /// False for a file that only holds shared variables and styles.
    pub is_widget: bool,
    pub name: String,
    pub size: [f32; 2],
    /// The parts as last read successfully (what is on screen), with every option at its default.
    pub parts: Vec<Part>,
    /// What the file lets its user adjust.
    #[serde(default)]
    pub options: Vec<WidgetOption>,
    /// The script that feeds this widget, what it asks to do, and the seconds between its runs.
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub every: u32,
    /// What is wrong with the file as it is now, if anything.
    pub error: Option<String>,
    pub error_line: Option<usize>,
}

/// Whether `name` is a plain file name Lumen will read or write in the widgets folder.
pub fn is_file_name(name: &str) -> bool {
    let stem = name.strip_suffix(".yaml").or_else(|| name.strip_suffix(".yml")).unwrap_or("");
    !stem.is_empty() && stem.len() <= 64 && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A file name for a widget called `name`: "Big clock!" → `big-clock.yaml`.
pub fn file_name_for(name: &str) -> String {
    let slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug = slug.split('-').filter(|piece| !piece.is_empty()).collect::<Vec<_>>().join("-");
    format!("{}.yaml", if slug.is_empty() { "widget" } else { &slug[..slug.len().min(48)] })
}

/// A starting point for a new widget file.
pub fn starter(name: &str) -> String {
    let parts = vec![
        Part { id: "title".into(), name: "Title".into(), text: name.to_owned(), x: 16.0, y: 14.0, width: 188.0, height: 20.0, stretch: true, size: 15.0, weight: PartWeight::SemiBold, ..Default::default() },
        Part { id: "time".into(), name: "Time".into(), text: "{clock.time} {clock.suffix}".into(), x: 16.0, y: 40.0, width: 188.0, height: 34.0, stretch: true, size: 26.0, color: PartColor::Highlight, ..Default::default() },
        Part { id: "note".into(), name: "Note".into(), text: "CPU {stats.cpu}  ·  RAM {stats.memory}".into(), x: 16.0, y: 80.0, width: 188.0, height: 18.0, stretch: true, color: PartColor::Muted, ..Default::default() },
    ];
    render(name, DEFAULT_SIZE, &parts, None, &|_| None)
}

/// Reads a widget file. `include` fetches the text of another file in the
/// same folder by name (for `include:`).
pub fn parse(text: &str, include: &dyn Fn(&str) -> Option<String>) -> Result<WidgetFile, FileError> {
    parse_with(text, include, &OptionValues::new())
}

/// Reads a widget file with one widget's answers to its options (anything
/// not answered, or answered with something the option does not allow, is
/// at its default).
pub fn parse_with(text: &str, include: &dyn Fn(&str) -> Option<String>, answers: &OptionValues) -> Result<WidgetFile, FileError> {
    let text = text.trim_start_matches('\u{feff}');
    let doc = read(text)?;
    if doc.get("lumen-widget").is_none() {
        return Err(FileError { message: "this is not a widget file: it must have the line `lumen-widget: 1`".into(), line: Some(1) });
    }
    let (mut variables, styles) = shared(&doc, include).map_err(|error| at_key(error, text, "include"))?;
    // Options are variables whose value the user chooses.
    let options = options_of(&doc).map_err(|error| at_key(error, text, "options"))?;
    for option in &options {
        let value = answers.get(&option.name).filter(|answer| allowed(option, answer)).unwrap_or(&option.default);
        variables.insert(Value::String(option.name.clone()), serde_yaml_ng::to_value(tidy(option, value)).unwrap_or(Value::Null));
    }

    let name = doc.get("name").and_then(Value::as_str).map(str::trim).filter(|name| !name.is_empty()).unwrap_or("Widget").chars().take(60).collect();
    let size = match doc.get("size") {
        None => DEFAULT_SIZE,
        Some(value) => {
            let value = substitute(value.clone(), &variables).map_err(|error| at_key(error, text, "size"))?;
            let (width, height) = pair(&value).ok_or_else(|| at_key(FileError::new("`size` must be two numbers, like `size: [260, 120]`"), text, "size"))?;
            (width.clamp(MIN_SIZE.0, 2000.0), height.clamp(MIN_SIZE.1, 2000.0))
        }
    };

    let listed = match doc.get("parts") {
        None | Some(Value::Null) => &Vec::new(),
        Some(Value::Sequence(parts)) => parts,
        Some(_) => return Err(at_key(FileError::new("`parts` must be a list, each item starting with `- id: …`"), text, "parts")),
    };
    let mut parts = Vec::with_capacity(listed.len());
    for (index, raw) in listed.iter().enumerate() {
        let id = raw.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
        let part = part_from(raw, &variables, &styles).map_err(|mut error| {
            error.message = if id.is_empty() { format!("part {}: {}", index + 1, error.message) } else { format!("part \"{id}\": {}", error.message) };
            error.line = part_line(text, &id).or_else(|| key_line(text, "parts"));
            error
        })?;
        if parts.iter().any(|other: &Part| other.id == part.id) {
            return Err(FileError { message: format!("two parts are both called \"{}\"; each part needs its own id", part.id), line: part_line(text, &part.id) });
        }
        parts.push(part);
    }
    let count = parts.len();
    let parts = sanitise_parts(parts);
    if parts.len() != count {
        return Err(FileError::new("a part's id may only use letters, numbers and dashes, and a widget holds at most 32 parts"));
    }
    let (script, permissions, every) = script_of(&doc, text)?;
    Ok(WidgetFile { options, name, size, parts, script, permissions, every })
}

/// Reads the `script:`, `permissions:` and `every:` settings.
fn script_of(doc: &Value, text: &str) -> Result<(Option<String>, Permissions, u32), FileError> {
    let script = match doc.get("script") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) if is_script_name(name.trim()) => Some(name.trim().to_owned()),
        Some(_) => return Err(at_key(FileError::new("`script` must be the name of a .luau file in the widgets folder, like `script: my-widget.luau` (letters, numbers and dashes only)"), text, "script")),
    };
    let mut permissions = Permissions::default();
    let asked: Vec<&Value> = match doc.get("permissions") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Sequence(list)) => list.iter().collect(),
        Some(one @ Value::String(_)) => vec![one],
        Some(_) => return Err(at_key(FileError::new("`permissions` must be a list, like `permissions: [files, network]`"), text, "permissions")),
    };
    for value in asked {
        match value.as_str().map(str::trim) {
            Some("files") => permissions.files = true,
            Some("network") => permissions.network = true,
            Some("run") => permissions.run = true,
            _ => return Err(at_key(FileError::new("a permission is one of `files` (read and write files), `network` (go online) or `run` (run and open programs)"), text, "permissions")),
        }
    }
    let every = match doc.get("every") {
        None | Some(Value::Null) => DEFAULT_EVERY,
        Some(value) => match value.as_f64() {
            Some(seconds) if seconds >= f64::from(EVERY_RANGE.0) && seconds <= f64::from(EVERY_RANGE.1) => seconds.round() as u32,
            _ => return Err(at_key(FileError::new("`every` is the number of seconds between the script's runs, from 1 to 3600"), text, "every")),
        },
    };
    if script.is_none() && permissions != Permissions::default() {
        return Err(at_key(FileError::new("`permissions` are for a script, and this widget names none; add `script: something.luau` or remove the line"), text, "permissions"));
    }
    Ok((script, permissions, every))
}

/// Points a widget file's `script:` at another file name, leaving the rest of the text as it was.
pub fn rename_script(text: &str, to: &str) -> String {
    let lines: Vec<String> = text.lines().map(|line| if line.starts_with("script") && line["script".len()..].trim_start().starts_with(':') { format!("script: {to}") } else { line.to_owned() }).collect();
    lines.join("\n") + if text.ends_with('\n') { "\n" } else { "" }
}

/// The files a widget file includes, as written (not checked to exist).
pub fn includes(text: &str) -> Vec<String> {
    let Ok(doc) = read(text.trim_start_matches('\u{feff}')) else { return Vec::new() };
    match doc.get("include") {
        Some(Value::String(name)) => vec![name.clone()],
        Some(Value::Sequence(names)) => names.iter().filter_map(|name| name.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// Points a widget file's `include:` at another file name. Only the
/// `include` setting is touched (and any list under it); the rest of the
/// text, comments included, stays as it was.
pub fn rename_include(text: &str, from: &str, to: &str) -> String {
    let mut inside = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let top_level = !line.starts_with([' ', '\t', '-', '#']) && !line.trim().is_empty();
            if top_level {
                inside = line.starts_with("include") && line["include".len()..].trim_start().starts_with(':');
            }
            if inside { line.replace(from, to) } else { line.to_owned() }
        })
        .collect();
    lines.join("\n") + if text.ends_with('\n') { "\n" } else { "" }
}

/// Whether a file is a widget (has `lumen-widget:`) rather than only shared
/// variables and styles. A file that cannot be read at all counts as a widget
/// if it mentions the key, so its mistake is reported.
pub fn is_widget(text: &str) -> bool {
    match read(text.trim_start_matches('\u{feff}')) {
        Ok(doc) => doc.get("lumen-widget").is_some(),
        Err(_) => text.contains("lumen-widget"),
    }
}

/// Writes a widget back as a file. With the file's `previous` text, anything
/// the author wrote that still holds is kept as written: the `include`,
/// `variables` and `styles` sections, and each part setting whose value has
/// not changed (so `size: $big` stays `$big`). Comments are not kept.
pub fn render(name: &str, size: (f32, f32), parts: &[Part], previous: Option<&str>, include: &dyn Fn(&str) -> Option<String>) -> String {
    render_with(name, size, parts, previous, include, &OptionValues::new())
}

/// As [`render`], for parts that were worked out with one widget's answers
/// to the file's options: the comparison with the old file uses the same
/// answers, so a setting written `$big` stays `$big` whatever that widget chose.
pub fn render_with(name: &str, size: (f32, f32), parts: &[Part], previous: Option<&str>, include: &dyn Fn(&str) -> Option<String>, answers: &OptionValues) -> String {
    let old_doc = previous.and_then(|text| read(text.trim_start_matches('\u{feff}')).ok());
    let old_parts = previous.and_then(|text| parse_with(text, include, answers).ok()).map(|file| file.parts).unwrap_or_default();
    let old_raw: Vec<&Value> = old_doc.as_ref().and_then(|doc| doc.get("parts")).and_then(Value::as_sequence).map(|list| list.iter().collect()).unwrap_or_default();

    let mut out = String::from("# A Lumen widget. Colours are theme roles (text, muted, highlight, primary, accent…), never hex values.\n");
    out.push_str("lumen-widget: 1\n");
    out.push_str(&format!("name: {}\n", scalar(&Value::String(name.to_owned()))));
    // Keep `size: $something` if it still works out to the same size.
    let old_size = old_doc.as_ref().and_then(|doc| doc.get("size")).filter(|_| previous.and_then(|text| parse(text, include).ok()).is_some_and(|file| file.size == size));
    match old_size {
        Some(raw) => emit("size", raw, 0, &mut out),
        None => out.push_str(&format!("size: [{}, {}]\n", number(f64::from(size.0)), number(f64::from(size.1)))),
    }
    if let Some(doc) = &old_doc {
        for key in ["include", "script", "permissions", "every", "options", "variables", "styles"] {
            if let Some(value) = doc.get(key).filter(|value| !value.is_null()) {
                out.push('\n');
                emit(key, value, 0, &mut out);
            }
        }
    }

    out.push_str("\nparts:\n");
    for part in parts {
        let old = old_parts.iter().find(|old| old.id == part.id);
        let raw = old.and_then(|_| old_raw.iter().find(|raw| raw.get("id").and_then(Value::as_str) == Some(part.id.as_str())).and_then(|raw| raw.as_mapping()));
        let (now, before, fresh) = (literal(part), old.map(literal), literal(&Part { id: part.id.clone(), name: part.id.clone(), ..Default::default() }));
        let mut first = true;
        for key in PART_KEYS {
            let written = match (raw, &before) {
                // A part that was in the file: keep what the author wrote unless its value changed.
                (Some(raw), Some(before)) if key == "style" || before.get(key) == now.get(key) => raw_setting(raw, key).cloned(),
                (Some(_), Some(_)) => now.get(key).cloned(),
                // A new part: write only what differs from a plain part.
                _ => now.get(key).filter(|value| key == "id" || fresh.get(key) != Some(*value)).cloned(),
            };
            // `shown: $option` is `hidden` the other way round; if hidden did not change, keep it as written.
            let kept_shown = raw.zip(before.as_ref()).filter(|(raw, before)| key == "hidden" && before.get(key) == now.get(key) && raw.get("hidden").is_none()).and_then(|(raw, _)| raw.get("shown").cloned());
            let (key, written) = match kept_shown {
                Some(value) => ("shown", Some(value)),
                None => (key, written),
            };
            if let Some(value) = written {
                out.push_str(if first { "  - " } else { "    " });
                first = false;
                emit(key, &value, 4, &mut out);
            }
        }
    }
    out
}

// ----- reading -----

fn read(text: &str) -> Result<Value, FileError> {
    let doc: Value = serde_yaml_ng::from_str(text).map_err(|error| {
        let line = error.location().map(|at| at.line());
        // The library's message ends with its own "at line…"; ours carries the line separately.
        let message = error.to_string();
        let message = message.split(" at line ").next().unwrap_or(&message).to_owned();
        FileError { message: format!("this is not valid YAML: {message}"), line }
    })?;
    match doc {
        Value::Mapping(_) => Ok(doc),
        Value::Null => Ok(Value::Mapping(Mapping::new())),
        _ => Err(FileError { message: "the file must be a list of settings such as `name:` and `parts:`".into(), line: Some(1) }),
    }
}

/// Reads the `options:` section.
fn options_of(doc: &Value) -> Result<Vec<WidgetOption>, FileError> {
    let listed = match doc.get("options") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Mapping(listed)) => listed,
        Some(_) => return Err(FileError::new("`options` must be a group of named options, each with a `type`")),
    };
    let mut options = Vec::new();
    for (name, settings) in listed {
        let name = name.as_str().unwrap_or_default().to_owned();
        let wrong = |why: String| FileError::new(format!("option `{name}`: {why}"));
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(FileError::new(format!("`{name}` cannot be an option's name; use letters, numbers and dashes")));
        }
        let Value::Mapping(settings) = settings else { return Err(wrong("must be a group of settings, like `{ label: Size, type: number, default: 40 }`".into())) };
        let text = |key: &str| settings.get(key).and_then(Value::as_str).map(str::to_owned);
        let kind = match text("type").unwrap_or_default().to_lowercase().as_str() {
            "number" => OptionKind::Number,
            "toggle" | "switch" | "on-off" => OptionKind::Toggle,
            "text" => OptionKind::Text,
            "choice" => OptionKind::Choice,
            "color" | "colour" => OptionKind::Color,
            other => return Err(wrong(format!("`type: {other}` is not a type of option; use number, toggle, text, choice or color"))),
        };
        let choices: Vec<String> = settings.get("choices").and_then(Value::as_sequence).map(|list| list.iter().filter_map(|item| item.as_str().map(str::to_owned).or_else(|| item.as_f64().map(number))).collect()).unwrap_or_default();
        if kind == OptionKind::Choice && choices.is_empty() {
            return Err(wrong("a choice needs a list of `choices`".into()));
        }
        let (min, max) = (settings.get("min").and_then(Value::as_f64), settings.get("max").and_then(Value::as_f64));
        if min.zip(max).is_some_and(|(min, max)| min > max) {
            return Err(wrong("its `min` is larger than its `max`".into()));
        }
        let mut option = WidgetOption { label: text("label").unwrap_or_else(|| name.clone()), name: name.clone(), kind, default: serde_json::Value::Null, min, max, choices };
        // What it is before anyone changes it: the file's default, or the plainest value of its type.
        let fallback = match kind {
            OptionKind::Number => serde_json::json!(min.unwrap_or(0.0)),
            OptionKind::Toggle => serde_json::json!(false),
            OptionKind::Text => serde_json::json!(""),
            OptionKind::Choice => serde_json::json!(option.choices[0]),
            OptionKind::Color => serde_json::json!("text"),
        };
        let given = settings.get("default").and_then(|value| serde_json::to_value(value).ok()).map(|value| if let (OptionKind::Choice | OptionKind::Text, Some(n)) = (kind, value.as_f64()) { serde_json::json!(number(n)) } else { value });
        option.default = match given {
            Some(value) if allowed(&option, &value) => tidy(&option, &value),
            Some(_) => return Err(wrong("its `default` is not something the option allows".into())),
            None => fallback,
        };
        options.push(option);
        if options.len() > MAX_OPTIONS {
            return Err(FileError::new(format!("a widget can expose at most {MAX_OPTIONS} options")));
        }
    }
    Ok(options)
}

/// Whether an answer is something the option can be.
fn allowed(option: &WidgetOption, answer: &serde_json::Value) -> bool {
    match option.kind {
        OptionKind::Number => answer.as_f64().is_some_and(f64::is_finite),
        OptionKind::Toggle => answer.is_boolean(),
        OptionKind::Text => answer.as_str().is_some_and(|text| text.chars().count() <= 200),
        OptionKind::Choice => answer.as_str().is_some_and(|text| option.choices.iter().any(|choice| choice == text)),
        OptionKind::Color => answer.as_str().is_some_and(|name| colour(name).is_ok()),
    }
}

/// An allowed answer, kept inside the option's range.
fn tidy(option: &WidgetOption, answer: &serde_json::Value) -> serde_json::Value {
    match (option.kind, answer.as_f64()) {
        (OptionKind::Number, Some(value)) => serde_json::json!(value.clamp(option.min.unwrap_or(f64::MIN), option.max.unwrap_or(f64::MAX))),
        _ => answer.clone(),
    }
}

type Shared = (Mapping, Mapping);

/// The variables and styles a file can use: those of its includes, then its own on top.
fn shared(doc: &Value, include: &dyn Fn(&str) -> Option<String>) -> Result<Shared, FileError> {
    let (mut variables, mut styles) = (Mapping::new(), Mapping::new());
    let names: Vec<String> = match doc.get("include") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(name)) => vec![name.clone()],
        Some(Value::Sequence(names)) => names.iter().filter_map(|name| name.as_str().map(str::to_owned)).collect(),
        Some(_) => return Err(FileError::new("`include` must be a file name or a list of file names")),
    };
    for name in names {
        if !is_file_name(&name) {
            return Err(FileError::new(format!("`include: {name}` must be the plain name of a .yaml file in the widgets folder")));
        }
        let text = include(&name).ok_or_else(|| FileError::new(format!("the included file {name} is not in the widgets folder")))?;
        let other = read(text.trim_start_matches('\u{feff}')).map_err(|error| FileError::new(format!("the included file {name} has a mistake{}: {}", error.line.map_or(String::new(), |line| format!(" on line {line}")), error.message)))?;
        merge(&mut variables, other.get("variables"));
        merge(&mut styles, other.get("styles"));
    }
    merge(&mut variables, doc.get("variables"));
    merge(&mut styles, doc.get("styles"));
    Ok((variables, styles))
}

fn merge(into: &mut Mapping, from: Option<&Value>) {
    if let Some(Value::Mapping(from)) = from {
        for (key, value) in from {
            into.insert(key.clone(), value.clone());
        }
    }
}

/// Replaces every value that is exactly `$name` with that variable's value.
fn substitute(value: Value, variables: &Mapping) -> Result<Value, FileError> {
    Ok(match value {
        Value::String(text) => match text.strip_prefix('$') {
            Some(name) if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') => {
                variables.get(name).cloned().ok_or_else(|| FileError::new(format!("there is no variable called `{name}`; add it under `variables:`")))?
            }
            _ => Value::String(text),
        },
        Value::Sequence(items) => Value::Sequence(items.into_iter().map(|item| substitute(item, variables)).collect::<Result<_, _>>()?),
        Value::Mapping(map) => {
            let mut out = Mapping::new();
            for (key, value) in map {
                out.insert(key, substitute(value, variables)?);
            }
            Value::Mapping(out)
        }
        other => other,
    })
}

/// A part setting by its usual name or one of its other spellings.
fn raw_setting<'a>(raw: &'a Mapping, key: &str) -> Option<&'a Value> {
    let other = match key {
        "color" => "colour",
        "from-bottom" => "fromBottom",
        "alert-when" => "alertWhen",
        "alert-color" => "alert-colour",
        "hover-only" => "hoverOnly",
        _ => "",
    };
    raw.get(key).or_else(|| raw.get(other))
}

fn part_from(raw: &Value, variables: &Mapping, styles: &Mapping) -> Result<Part, FileError> {
    let Value::Mapping(raw) = raw else { return Err(FileError::new("must be a group of settings starting with `- id: …`")) };
    // The shared style first, then the part's own settings on top.
    let mut settings = Mapping::new();
    if let Some(style) = raw.get("style") {
        let name = style.as_str().ok_or_else(|| FileError::new("`style` must be the name of a style"))?;
        let Some(Value::Mapping(shared)) = styles.get(name) else { return Err(FileError::new(format!("there is no style called `{name}`; add it under `styles:`"))) };
        for (key, value) in shared {
            settings.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in raw {
        settings.insert(key.clone(), value.clone());
    }
    let Value::Mapping(settings) = substitute(Value::Mapping(settings), variables)? else { unreachable!("a mapping stays a mapping") };

    let mut part = Part::default();
    for (key, value) in &settings {
        let key = key.as_str().unwrap_or_default();
        let text = || value.as_str().map(str::to_owned).or_else(|| value.as_f64().map(number)).ok_or_else(|| FileError::new(format!("`{key}` must be text")));
        let num = || value.as_f64().map(|n| n as f32).ok_or_else(|| FileError::new(format!("`{key}` must be a number")));
        let flag = || value.as_bool().ok_or_else(|| FileError::new(format!("`{key}` must be true or false")));
        let two = || pair(value).ok_or_else(|| FileError::new(format!("`{key}` must be two numbers, like `{key}: [16, 20]`")));
        match key {
            "id" => part.id = text()?,
            "name" => part.name = text()?,
            "style" => {}
            "kind" => {
                part.kind = match text()?.to_lowercase().as_str() {
                    "text" => PartKind::Text,
                    "bar" => PartKind::Bar,
                    "image" | "picture" => PartKind::Image,
                    "shape" => PartKind::Shape,
                    "ring" => PartKind::Ring,
                    "graph" => PartKind::Graph,
                    "icon" => PartKind::Icon,
                    other => return Err(FileError::new(format!("`kind: {other}` is not a kind of part; use text, bar, image, shape, ring, graph or icon"))),
                }
            }
            "text" => part.text = text()?,
            "source" => part.source = text()?,
            "font" => part.font = Some(text()?),
            "size" => part.size = num()?,
            "weight" => {
                part.weight = match text()?.to_lowercase().replace(['-', ' '], "").as_str() {
                    "regular" | "normal" => PartWeight::Regular,
                    "medium" => PartWeight::Medium,
                    "bold" | "semibold" => PartWeight::SemiBold,
                    other => return Err(FileError::new(format!("`weight: {other}` is not a weight; use regular, medium or bold"))),
                }
            }
            "color" | "colour" => part.color = colour(&text()?)?,
            "opacity" => part.opacity = num()?.clamp(0.0, 100.0) as u8,
            "align" => {
                part.align = match text()?.to_lowercase().as_str() {
                    "left" => PartAlign::Left,
                    "centre" | "center" => PartAlign::Centre,
                    "right" => PartAlign::Right,
                    other => return Err(FileError::new(format!("`align: {other}` is not a way to align; use left, centre or right"))),
                }
            }
            "at" => (part.x, part.y) = two()?,
            "box" => (part.width, part.height) = two()?,
            "stretch" => part.stretch = flag()?,
            "from-bottom" | "fromBottom" => part.from_bottom = flag()?,
            "radius" => part.radius = num()?,
            "hidden" => part.hidden = flag()?,
            // The other way round, for an on/off option: `shown: $show-date`.
            "shown" => part.hidden = !flag()?,
            "click" => {
                part.click = text()?;
                if crate::behaviour::parse_action(&part.click).is_none() && !part.click.trim().is_empty() {
                    return Err(FileError::new(format!("`click: {}` is not something a click can do; use `open <program, file or web page>`, `media.toggle`, `lumen.open` or `script`", part.click)));
                }
            }
            "when" | "alert-when" | "alertWhen" => {
                let condition = text()?;
                crate::behaviour::check_condition(&condition).map_err(|why| FileError::new(format!("`{key}`: {why}")))?;
                if key == "when" { part.when = condition } else { part.alert_when = condition }
            }
            "alert-color" | "alert-colour" | "alertColor" => part.alert_color = colour(&text()?)?,
            "hover-only" | "hoverOnly" => part.hover_only = flag()?,
            other => return Err(FileError::new(format!("`{other}` is not a setting a part has. The settings are: {}", PART_KEYS.join(", ")))),
        }
    }
    if part.id.is_empty() {
        return Err(FileError::new("needs an `id`, such as `- id: title`"));
    }
    if part.name.is_empty() {
        part.name = part.id.clone();
    }
    Ok(part)
}

fn colour(name: &str) -> Result<PartColor, FileError> {
    Ok(match name.to_lowercase().replace(['-', '_', ' '], "").as_str() {
        "text" => PartColor::Text,
        "muted" => PartColor::Muted,
        "highlight" => PartColor::Highlight,
        "primary" => PartColor::Primary,
        "secondary" => PartColor::Secondary,
        "tertiary" => PartColor::Tertiary,
        "neutral" => PartColor::Neutral,
        "surface" => PartColor::Surface,
        "onsurface" => PartColor::OnSurface,
        "outline" => PartColor::Outline,
        "accent" => PartColor::Accent,
        _ => {
            let hint = if name.starts_with('#') || name.starts_with("rgb") { " Lumen widgets do not hold colours of their own, so they always match the theme." } else { "" };
            return Err(FileError::new(format!("`color: {name}` is not a theme colour.{hint} Use text, muted, highlight, primary, secondary, tertiary, neutral, surface, on-surface, outline or accent")));
        }
    })
}

fn pair(value: &Value) -> Option<(f32, f32)> {
    let items = value.as_sequence()?;
    match items.as_slice() {
        [a, b] => Some((a.as_f64()? as f32, b.as_f64()? as f32)),
        _ => None,
    }
}

/// The line (1-based) where the part with this id starts.
fn part_line(text: &str, id: &str) -> Option<usize> {
    if id.is_empty() {
        return None;
    }
    text.lines().position(|line| {
        let line = line.split('#').next().unwrap_or_default().trim().trim_start_matches('-').trim();
        line.strip_prefix("id:").is_some_and(|rest| rest.trim().trim_matches(['"', '\'']) == id)
    })
    .map(|index| index + 1)
}

/// The line (1-based) of a top-level setting.
fn key_line(text: &str, key: &str) -> Option<usize> {
    text.lines().position(|line| line.starts_with(key) && line[key.len()..].trim_start().starts_with(':')).map(|index| index + 1)
}

fn at_key(mut error: FileError, text: &str, key: &str) -> FileError {
    error.line = error.line.or_else(|| key_line(text, key));
    error
}

// ----- writing -----

/// A part as the settings a file would hold, every one spelled out.
fn literal(part: &Part) -> Mapping {
    let mut map = Mapping::new();
    let mut set = |key: &str, value: Value| {
        map.insert(Value::String(key.to_owned()), value);
    };
    let num = |value: f32| serde_yaml_ng::to_value(f64::from(value)).unwrap_or(Value::Null);
    let pair = |a: f32, b: f32| Value::Sequence(vec![num(a), num(b)]);
    set("id", Value::String(part.id.clone()));
    set("name", Value::String(part.name.clone()));
    set("kind", Value::String(match part.kind { PartKind::Text => "text", PartKind::Bar => "bar", PartKind::Image => "image", PartKind::Shape => "shape", PartKind::Ring => "ring", PartKind::Graph => "graph", PartKind::Icon => "icon" }.to_owned()));
    set("text", Value::String(part.text.clone()));
    set("source", Value::String(part.source.clone()));
    set("font", part.font.clone().map_or(Value::Null, Value::String));
    set("size", num(part.size));
    set("weight", Value::String(match part.weight { PartWeight::Regular => "regular", PartWeight::Medium => "medium", PartWeight::SemiBold => "bold" }.to_owned()));
    set("color", Value::String(colour_name(part.color)));
    set("opacity", serde_yaml_ng::to_value(part.opacity).unwrap_or(Value::Null));
    set("align", Value::String(match part.align { PartAlign::Left => "left", PartAlign::Centre => "centre", PartAlign::Right => "right" }.to_owned()));
    set("at", pair(part.x, part.y));
    set("box", pair(part.width, part.height));
    set("stretch", Value::Bool(part.stretch));
    set("from-bottom", Value::Bool(part.from_bottom));
    set("radius", num(part.radius));
    set("hidden", Value::Bool(part.hidden));
    set("click", Value::String(part.click.clone()));
    set("when", Value::String(part.when.clone()));
    set("alert-when", Value::String(part.alert_when.clone()));
    set("alert-color", Value::String(colour_name(part.alert_color)));
    set("hover-only", Value::Bool(part.hover_only));
    map
}

/// A colour choice as a file spells it.
fn colour_name(colour: PartColor) -> String {
    let name = serde_json::to_value(colour).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default();
    if name == "onSurface" { "on-surface".to_owned() } else { name }
}

fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 { format!("{}", value as i64) } else { format!("{value}") }
}

/// One value on one line: a scalar, or a short list or group in brackets.
fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), number),
        Value::String(text) => {
            let plain = !text.is_empty()
                && text.trim() == text
                && !text.chars().any(|c| ":#{}[],&*!|>'\"%@`\\\n\t".contains(c))
                && !text.starts_with(['-', '?', ' '])
                && serde_yaml_ng::from_str::<Value>(text).is_ok_and(|parsed| parsed.is_string());
            if plain { text.clone() } else { format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")) }
        }
        Value::Sequence(items) => format!("[{}]", items.iter().map(scalar).collect::<Vec<_>>().join(", ")),
        Value::Mapping(map) => format!("{{ {} }}", map.iter().map(|(key, value)| format!("{}: {}", scalar(key), scalar(value))).collect::<Vec<_>>().join(", ")),
        Value::Tagged(tagged) => scalar(&tagged.value),
    }
}

/// Writes `key: value`, with a group of groups (variables, styles) on lines of its own.
fn emit(key: &str, value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Mapping(map) if indent == 0 => {
            out.push_str(&format!("{key}:\n"));
            for (name, inner) in map {
                out.push_str(&format!("  {}: {}\n", scalar(name), scalar(inner)));
            }
        }
        _ => out.push_str(&format!("{key}: {}\n", scalar(value))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARED: &str = "variables:\n  display: Space Grotesk\n  big: 40\nstyles:\n  caption: { font: Inter, size: 13, color: muted }\n";
    const CLOCK: &str = r#"lumen-widget: 1
name: Big clock
size: [260, 120]
include: shared.yaml
variables:
  big: 56          # overrides the shared one
parts:
  - id: time
    text: "{clock.time}"
    font: $display
    size: $big
    weight: bold
    color: accent
    at: [20, 28]
    box: [220, 64]
    stretch: true
  - id: date
    text: "{clock.date}"
    style: caption
    at: [20, 14]
    box: [220, 18]
  - id: cpu
    kind: bar
    source: stats.cpu
    colour: on-surface
    at: [20, 100]
    box: [220, 5]
    from-bottom: true
"#;

    fn include(name: &str) -> Option<String> {
        (name == "shared.yaml").then(|| SHARED.to_owned())
    }

    #[test]
    fn a_file_becomes_parts_with_variables_styles_and_includes_worked_out() {
        let file = parse(CLOCK, &include).unwrap();
        assert_eq!((file.name.as_str(), file.size), ("Big clock", (260.0, 120.0)));
        let [time, date, cpu] = file.parts.as_slice() else { panic!("three parts") };
        assert_eq!((time.font.as_deref(), time.size, time.weight, time.color), (Some("Space Grotesk"), 56.0, PartWeight::SemiBold, PartColor::Accent), "the file's own variable wins over the included one");
        assert_eq!((time.x, time.y, time.width, time.height, time.stretch), (20.0, 28.0, 220.0, 64.0, true));
        assert_eq!((date.font.as_deref(), date.size, date.color), (Some("Inter"), 13.0, PartColor::Muted), "from the shared style");
        assert_eq!((cpu.kind, cpu.source.as_str(), cpu.color, cpu.from_bottom), (PartKind::Bar, "stats.cpu", PartColor::OnSurface, true));
        assert_eq!(time.name, "time", "a part is called by its id unless it says otherwise");
    }

    #[test]
    fn mistakes_are_explained_with_a_line_to_look_at() {
        let wrong = |text: &str| parse(text, &include).unwrap_err();
        let hex = wrong(&CLOCK.replace("color: accent", "color: \"#ff0000\""));
        assert!(hex.message.contains("part \"time\"") && hex.message.contains("not a theme colour") && hex.message.contains("always match the theme"), "{}", hex.message);
        assert_eq!(hex.line, Some(8), "the line where that part starts");

        let variable = wrong(&CLOCK.replace("size: $big", "size: $huge"));
        assert!(variable.message.contains("no variable called `huge`"), "{}", variable.message);
        let style = wrong(&CLOCK.replace("style: caption", "style: heading"));
        assert!(style.message.contains("no style called `heading`") && style.line == Some(17), "{style:?}");
        let typo = wrong(&CLOCK.replace("stretch: true", "strech: true"));
        assert!(typo.message.contains("`strech` is not a setting"), "{}", typo.message);
        let twice = wrong(&CLOCK.replace("- id: date", "- id: time"));
        assert!(twice.message.contains("both called \"time\""), "{}", twice.message);

        let syntax = wrong("lumen-widget: 1\nparts:\n  - id: a\n   text: oops\n");
        assert!(syntax.message.starts_with("this is not valid YAML") && syntax.line.is_some(), "{syntax:?}");
        assert!(wrong("name: Just a file").message.contains("lumen-widget: 1"));
        assert!(wrong(&CLOCK.replace("include: shared.yaml", "include: gone.yaml")).message.contains("gone.yaml is not in the widgets folder"));
        assert!(wrong(&CLOCK.replace("include: shared.yaml", "include: ../secret.yaml")).message.contains("plain name"));
    }

    #[test]
    fn writing_back_keeps_what_the_author_wrote_where_nothing_changed() {
        let mut parts = parse(CLOCK, &include).unwrap().parts;
        parts[1].x = 44.0; // the designer moved the date
        parts.push(Part { id: "dot".into(), name: "Dot".into(), kind: PartKind::Shape, x: 5.0, y: 6.0, width: 8.0, height: 8.0, radius: 4.0, color: PartColor::Primary, ..Default::default() });
        let text = render("Big clock", (260.0, 120.0), &parts, Some(CLOCK), &include);

        assert!(text.contains("include: shared.yaml") && text.contains("variables:\n  big: 56"), "{text}");
        assert!(text.contains("size: $big") && text.contains("font: $display"), "untouched settings keep their variables:\n{text}");
        assert!(text.contains("style: caption") && text.contains("at: [44, 14]"), "the moved part keeps its style and gets its new place:\n{text}");
        assert!(!text.contains("font: Inter"), "what came from the style is not copied into the part:\n{text}");
        assert!(text.contains("- id: dot\n    name: Dot\n    kind: shape\n    color: primary\n    at: [5, 6]\n    box: [8, 8]\n    radius: 4\n"), "a new part lists only what is not the default:\n{text}");
        assert_eq!(parse(&text, &include).unwrap().parts, parts, "and the file reads back as the same parts");
    }

    #[test]
    fn a_changed_value_that_came_from_a_variable_becomes_a_plain_value() {
        let mut parts = parse(CLOCK, &include).unwrap().parts;
        parts[0].size = 48.0;
        let text = render("Big clock", (260.0, 120.0), &parts, Some(CLOCK), &include);
        assert!(text.contains("    size: 48\n") && text.contains("font: $display"), "{text}");
        assert_eq!(parse(&text, &include).unwrap().parts[0].size, 48.0);
    }

    #[test]
    fn a_new_file_is_tidy_and_reads_back() {
        let text = starter("My: widget");
        assert!(text.contains("name: \"My: widget\"") && text.contains("size: [220, 110]"), "{text}");
        let file = parse(&text, &|_| None).unwrap();
        assert_eq!((file.name.as_str(), file.parts.len()), ("My: widget", 3));
        assert_eq!(file.parts[1].text, "{clock.time} {clock.suffix}");
        assert!(!text.contains('#') || text.lines().filter(|line| line.contains('#')).all(|line| line.starts_with('#')), "no hex colours: only the comment line has a #");
    }

    #[test]
    fn behaviour_and_the_new_kinds_are_read_checked_and_written_back() {
        let text = r#"lumen-widget: 1
name: Busy
parts:
  - id: gauge
    kind: ring
    source: stats.cpu
    alert-when: stats.cpu > 90
    alert-color: accent
    at: [10, 10]
    box: [40, 40]
  - id: history
    kind: graph
    source: stats.memory
  - id: music
    kind: icon
    text: music
    click: media.toggle
    when: media.playing
    hover-only: true
  - id: web
    text: Open the forecast
    click: open https://open-meteo.com
"#;
        let file = parse(text, &|_| None).unwrap();
        let [gauge, history, music, web] = file.parts.as_slice() else { panic!("four parts") };
        assert_eq!((gauge.kind, gauge.alert_when.as_str(), gauge.alert_color), (PartKind::Ring, "stats.cpu > 90", PartColor::Accent));
        assert_eq!(history.kind, PartKind::Graph);
        assert_eq!((music.kind, music.text.as_str(), music.click.as_str(), music.when.as_str(), music.hover_only), (PartKind::Icon, "music", "media.toggle", "media.playing", true));
        assert_eq!(web.click, "open https://open-meteo.com");
        assert_eq!(parse(&render("Busy", file.size, &file.parts, Some(text), &|_| None), &|_| None).unwrap().parts, file.parts, "and it reads back the same");

        let wrong = |from: &str, to: &str| parse(&text.replace(from, to), &|_| None).unwrap_err().message;
        assert!(wrong("click: media.toggle", "click: format the disk").contains("not something a click can do"));
        assert!(wrong("when: media.playing", "when: is music on?").contains("not a condition"));
        assert!(wrong("alert-when: stats.cpu > 90", "alert-when: stats.cpu > hot").contains("not a number"));
        assert!(wrong("kind: ring", "kind: dial").contains("ring, graph or icon"));
    }

    const ADJUSTABLE: &str = r#"lumen-widget: 1
name: Adjustable
options:
  big: { label: Time size, type: number, default: 40, min: 20, max: 80 }
  show-date: { label: Show the date, type: toggle, default: true }
  tint: { label: Colour, type: color, default: accent }
  mood: { label: Mood, type: choice, choices: [calm, loud], default: calm }
  title: { type: text }
parts:
  - id: time
    text: "{clock.time}"
    size: $big
    color: $tint
  - id: date
    text: "{clock.date}"
    shown: $show-date
  - id: label
    text: $title
"#;

    #[test]
    fn a_file_exposes_options_that_each_widget_answers_for_itself() {
        let file = parse(ADJUSTABLE, &|_| None).unwrap();
        let described: Vec<(&str, &str, OptionKind)> = file.options.iter().map(|o| (o.name.as_str(), o.label.as_str(), o.kind)).collect();
        assert_eq!(described, [("big", "Time size", OptionKind::Number), ("show-date", "Show the date", OptionKind::Toggle), ("tint", "Colour", OptionKind::Color), ("mood", "Mood", OptionKind::Choice), ("title", "title", OptionKind::Text)]);
        assert_eq!((file.options[0].min, file.options[0].max, file.options[3].choices.len()), (Some(20.0), Some(80.0), 2));
        assert_eq!((file.parts[0].size, file.parts[0].color, file.parts[1].hidden, file.parts[2].text.as_str()), (40.0, PartColor::Accent, false, ""), "everything at its default");

        let answers: OptionValues = serde_json::from_str(r#"{ "big": 64, "show-date": false, "tint": "primary", "title": "Hi" }"#).unwrap();
        let mine = parse_with(ADJUSTABLE, &|_| None, &answers).unwrap();
        assert_eq!((mine.parts[0].size, mine.parts[0].color, mine.parts[1].hidden, mine.parts[2].text.as_str()), (64.0, PartColor::Primary, true, "Hi"));

        // Answers the option does not allow fall back to its default; numbers stay in range.
        let odd: OptionValues = serde_json::from_str(r##"{ "big": 500, "show-date": "yes", "tint": "#ff0000", "mood": "angry", "unknown": 1 }"##).unwrap();
        let tamed = parse_with(ADJUSTABLE, &|_| None, &odd).unwrap();
        assert_eq!((tamed.parts[0].size, tamed.parts[0].color, tamed.parts[1].hidden), (80.0, PartColor::Accent, false));
    }

    #[test]
    fn the_designer_keeps_options_in_place_whatever_a_widget_chose() {
        let answers: OptionValues = serde_json::from_str(r#"{ "big": 64, "tint": "primary" }"#).unwrap();
        let mut parts = parse_with(ADJUSTABLE, &|_| None, &answers).unwrap().parts;
        parts[1].x = 30.0;
        let text = render_with("Adjustable", DEFAULT_SIZE, &parts, Some(ADJUSTABLE), &|_| None, &answers);
        assert!(text.contains("size: $big") && text.contains("color: $tint") && text.contains("shown: $show-date"), "{text}");
        assert!(text.contains("options:\n  big: { label: Time size, type: number, default: 40, min: 20, max: 80 }"), "{text}");
        assert_eq!(parse(&text, &|_| None).unwrap().options.len(), 5);
        assert_eq!(parse_with(&text, &|_| None, &answers).unwrap().parts, parts);
    }

    #[test]
    fn badly_written_options_are_explained() {
        let wrong = |from: &str, to: &str| parse(&ADJUSTABLE.replace(from, to), &|_| None).unwrap_err();
        assert!(wrong("type: number", "type: slider").message.contains("not a type of option"));
        assert!(wrong("choices: [calm, loud], ", "").message.contains("needs a list of `choices`"));
        assert!(wrong("default: 40", "default: lots").message.contains("option `big`"));
        assert!(wrong("min: 20, max: 80", "min: 90, max: 80").message.contains("larger than"));
        assert_eq!(wrong("type: number", "type: slider").line, Some(3), "points at the options section");
    }

    #[test]
    fn includes_are_listed_and_can_be_renamed_without_touching_the_rest() {
        assert_eq!(includes(CLOCK), ["shared.yaml"]);
        assert_eq!(includes("lumen-widget: 1\ninclude: [a.yaml, b.yaml]\n"), ["a.yaml", "b.yaml"]);
        assert!(includes("lumen-widget: 1\n").is_empty());
        let renamed = rename_include(CLOCK, "shared.yaml", "mine-shared.yaml");
        assert!(renamed.contains("include: mine-shared.yaml") && renamed.contains("# overrides the shared one"), "{renamed}");
        assert_eq!(renamed.replace("mine-shared.yaml", "shared.yaml"), CLOCK, "nothing else changed");
        let listed = "lumen-widget: 1\ninclude:\n  - a.yaml\n  - b.yaml\nname: a.yaml fan\n";
        assert_eq!(rename_include(listed, "a.yaml", "c.yaml"), "lumen-widget: 1\ninclude:\n  - c.yaml\n  - b.yaml\nname: a.yaml fan\n", "only under include");
    }

    #[test]
    fn file_names_are_plain_and_made_from_the_widgets_name() {
        assert_eq!(file_name_for("Big clock!"), "big-clock.yaml");
        assert_eq!(file_name_for("  "), "widget.yaml");
        assert!(is_file_name("big-clock.yaml") && is_file_name("shared_1.yml"));
        for bad in ["../x.yaml", "a/b.yaml", "x.json", ".yaml", "has space.yaml", "c:\\x.yaml"] {
            assert!(!is_file_name(bad), "{bad}");
        }
        assert!(is_widget(CLOCK) && !is_widget(SHARED));
    }

    #[test]
    fn a_widget_can_name_a_script_and_what_it_asks_for() {
        let text = "lumen-widget: 1\nname: Stocks\nscript: stocks.luau\npermissions: [network, run]\nevery: 60\nparts:\n  - id: price\n    text: \"{script.price}\"\n    click: script\n";
        let file = parse(text, &|_| None).unwrap();
        assert_eq!((file.script.as_deref(), file.permissions, file.every), (Some("stocks.luau"), Permissions { files: false, network: true, run: true }, 60));
        assert_eq!(file.permissions.names(), ["network", "run"]);
        let plain = parse("lumen-widget: 1\nscript: a.luau\n", &|_| None).unwrap();
        assert_eq!((plain.permissions, plain.every), (Permissions::default(), DEFAULT_EVERY));
        assert_eq!(parse("lumen-widget: 1\n", &|_| None).unwrap().script, None);

        // Written back, the script settings are kept.
        let again = render(&file.name, file.size, &file.parts, Some(text), &|_| None);
        assert_eq!(parse(&again, &|_| None).unwrap(), file, "{again}");
        assert_eq!(parse(&rename_script(text, "stocks-2.luau"), &|_| None).unwrap().script.as_deref(), Some("stocks-2.luau"));
    }

    #[test]
    fn script_settings_with_mistakes_are_explained() {
        let wrong = |line: &str| parse(&format!("lumen-widget: 1\n{line}\n"), &|_| None).unwrap_err();
        for name in ["script: ../evil.luau", "script: C:/x.luau", "script: thing.exe", "script: [a]"] {
            let error = wrong(name);
            assert!(error.message.contains(".luau file in the widgets folder") && error.line == Some(2), "{name}: {error:?}");
        }
        assert!(wrong("script: a.luau\npermissions: [everything]").message.contains("a permission is one of"));
        assert!(wrong("script: a.luau\nevery: 0").message.contains("from 1 to 3600"));
        assert!(wrong("permissions: [files]").message.contains("this widget names none"));
    }

}
