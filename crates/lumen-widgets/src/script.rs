//! Scripted widgets: one sandboxed Luau machine per script.
//!
//! A script hands values to its widget's parts and can answer clicks:
//!
//! ```lua
//! function update()            -- called every few seconds
//!   return { greeting = "Hello", load = lumen.value("stats.cpu") }
//! end
//!
//! function click(part)         -- optional: a part with `click: script` was clicked
//!   lumen.open("https://example.com")
//! end
//! ```
//!
//! Whatever `update` returns is shown by parts as `{script.greeting}`,
//! `{script.load}`. Through the `lumen` library a script can do anything:
//! read and write files, fetch from the internet, run programs. Each of those
//! is a *permission* the widget file must list and the user must have agreed
//! to; without it the call fails with a plain message. Every run has a time
//! limit and the machine has a memory limit, so a broken script cannot hang
//! or swamp Lumen.

use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use lumen_ipc::widget_file::Permissions;
use mlua::{Function, Lua, Table, Value, VmState};

/// How long one call into a script may compute before it is stopped.
/// (Time spent waiting inside `lumen` calls, such as a download, has limits of its own.)
pub const RUN_LIMIT: Duration = Duration::from_secs(2);
/// The most memory one script's machine may hold.
pub const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
/// The largest file or download a script may read, and the most program output kept.
const MAX_READ: u64 = 4 * 1024 * 1024;
const MAX_OUTPUT: usize = 256 * 1024;
const NETWORK_TIMEOUT_MS: i32 = 10_000;
/// The most a web answer may hold.
const MAX_WEB_ANSWER: usize = 1024 * 1024;
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(15);
/// The most values one script may hand back, and how long each may be.
const MAX_VALUES: usize = 64;
const MAX_VALUE_CHARS: usize = 400;

/// The values a script handed back, by name.
pub type Values = HashMap<String, String>;

/// One script, loaded and ready to be called. Lives on the thread that made it.
pub struct Engine {
    lua: Lua,
    /// When the call now running must be over.
    deadline: Rc<Cell<Option<Instant>>>,
    /// The live values the script can ask for, replaced before each call.
    live: Rc<std::cell::RefCell<Values>>,
}

impl Engine {
    /// Loads a script. `name` is used in messages. The script's top level
    /// runs once here (under the time limit), defining its functions.
    pub fn new(name: &str, source: &str, permissions: Permissions) -> Result<Self, String> {
        let lua = Lua::new();
        let deadline: Rc<Cell<Option<Instant>>> = Rc::default();
        let live: Rc<std::cell::RefCell<Values>> = Rc::default();
        let failed = |error: mlua::Error| tidy(&error.to_string());

        lua.set_memory_limit(MEMORY_LIMIT).map_err(failed)?;
        let watch = deadline.clone();
        lua.set_interrupt(move |_| match watch.get() {
            Some(at) if Instant::now() > at => Err(mlua::Error::runtime("the script ran for too long and was stopped")),
            _ => Ok(VmState::Continue),
        });
        lua.globals().set("lumen", library(&lua, permissions, live.clone()).map_err(failed)?).map_err(failed)?;
        // From here the script cannot change the built-in libraries, only its own globals.
        lua.sandbox(true).map_err(failed)?;

        let engine = Self { lua, deadline, live };
        engine.timed(|lua| lua.load(source).set_name(name).exec())?;
        Ok(engine)
    }

    /// Runs `work` with the time limit set.
    fn timed<T>(&self, work: impl FnOnce(&Lua) -> mlua::Result<T>) -> Result<T, String> {
        self.deadline.set(Some(Instant::now() + RUN_LIMIT));
        let result = work(&self.lua);
        self.deadline.set(None);
        result.map_err(|error| tidy(&error.to_string()))
    }

    /// Calls the script's `update()` and returns what it handed back.
    /// A script without one hands back nothing.
    pub fn update(&self, live: &Values) -> Result<Values, String> {
        self.call("update", None, live)
    }

    /// Calls the script's `click(part)`, if it has one. What it returns (if
    /// anything) is added to the widget's values.
    pub fn click(&self, part: &str, live: &Values) -> Result<Values, String> {
        self.call("click", Some(part), live)
    }

    fn call(&self, function: &str, argument: Option<&str>, live: &Values) -> Result<Values, String> {
        self.live.replace(live.clone());
        self.timed(|lua| {
            let Ok(function) = lua.globals().get::<Function>(function) else { return Ok(Values::new()) };
            let returned = match argument {
                Some(text) => function.call::<Value>(text)?,
                None => function.call::<Value>(())?,
            };
            Ok(match returned {
                Value::Table(table) => values_of(&table),
                _ => Values::new(),
            })
        })
    }
}

/// Reads the table a script returned: text, numbers and true/false, by name.
fn values_of(table: &Table) -> Values {
    let mut values = Values::new();
    for (key, value) in table.pairs::<String, Value>().flatten() {
        let text = match value {
            Value::String(text) => text.to_string_lossy(),
            Value::Integer(number) => number.to_string(),
            Value::Number(number) => {
                if number.fract() == 0.0 && number.abs() < 1e15 { format!("{}", number as i64) } else { format!("{number:.2}") }
            }
            Value::Boolean(flag) => if flag { "yes".to_owned() } else { String::new() },
            _ => continue,
        };
        let name_ok = !key.is_empty() && key.len() <= 48 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if name_ok && values.len() < MAX_VALUES {
            values.insert(key, text.chars().take(MAX_VALUE_CHARS).collect());
        }
    }
    values
}

/// A script's error without the machine's own decoration.
fn tidy(message: &str) -> String {
    let first = message.lines().next().unwrap_or(message).trim();
    first.trim_start_matches("runtime error: ").trim_start_matches("syntax error: ").chars().take(300).collect()
}

/// The `lumen` library a script sees.
fn library(lua: &Lua, permissions: Permissions, live: Rc<std::cell::RefCell<Values>>) -> mlua::Result<Table> {
    let lumen = lua.create_table()?;
    let refused = |what: &str| mlua::Error::runtime(format!("this widget has not been allowed to {what}; add it under `permissions:` in the widget file and allow it in Lumen"));

    // Always there: Lumen's own live values, the time, and a log.
    lumen.set("value", lua.create_function(move |_, name: String| Ok(live.borrow().get(&name).cloned()))?)?;
    lumen.set("time", lua.create_function(|_, ()| Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())))?)?;
    lumen.set("log", lua.create_function(|_, text: String| {
        tracing::info!("script: {}", text.chars().take(500).collect::<String>());
        Ok(())
    })?)?;

    // JSON, which most web services speak: text to a table and back. Needs no permission.
    lumen.set("json_decode", lua.create_function(|lua, text: String| {
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|error| mlua::Error::runtime(format!("that is not JSON: {error}")))?;
        from_json(lua, &value, 0)
    })?)?;
    lumen.set("json_encode", lua.create_function(|_, value: Value| Ok(to_json(&value, 0)?.to_string()))?)?;

    // Files.
    let files = permissions.files;
    lumen.set("read_file", lua.create_function(move |_, path: String| {
        if !files {
            return Err(refused("read files"));
        }
        let size = std::fs::metadata(&path).map_err(|e| mlua::Error::runtime(format!("could not read {path}: {e}")))?.len();
        if size > MAX_READ {
            return Err(mlua::Error::runtime(format!("{path} is too large to read (over 4 MB)")));
        }
        std::fs::read_to_string(&path).map_err(|e| mlua::Error::runtime(format!("could not read {path}: {e}")))
    })?)?;
    lumen.set("write_file", lua.create_function(move |_, (path, text): (String, String)| {
        if !files {
            return Err(refused("write files"));
        }
        std::fs::write(&path, text).map_err(|e| mlua::Error::runtime(format!("could not write {path}: {e}")))
    })?)?;
    lumen.set("list_folder", lua.create_function(move |_, path: String| {
        if !files {
            return Err(refused("look in folders"));
        }
        let entries = std::fs::read_dir(&path).map_err(|e| mlua::Error::runtime(format!("could not open {path}: {e}")))?;
        Ok(entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).take(2000).collect::<Vec<String>>())
    })?)?;

    // The internet.
    let network = permissions.network;
    lumen.set("http_get", lua.create_function(move |_, url: String| {
        if !network {
            return Err(refused("go online"));
        }
        let answer = lumen_win::http::request(&lumen_win::http::Request { url: &url, method: "GET", headers: &[], body: None, timeout_ms: NETWORK_TIMEOUT_MS, max_bytes: MAX_WEB_ANSWER }).map_err(mlua::Error::runtime)?;
        if answer.status != 200 {
            return Err(mlua::Error::runtime(format!("{url} answered with an error ({})", answer.status)));
        }
        String::from_utf8(answer.body).map_err(|_| mlua::Error::runtime("the answer was not text"))
    })?)?;
    // Any kind of request: { url = …, method = "POST", headers = { Authorization = "Bearer …" }, body = "…" }.
    // Hands back { status = 200, body = "…" } whatever the status, so the script can tell a refusal from an answer.
    lumen.set("http_request", lua.create_function(move |lua, ask: Table| {
        if !network {
            return Err(refused("go online"));
        }
        let url: String = ask.get("url").map_err(|_| mlua::Error::runtime("http_request needs a url"))?;
        let method = ask.get::<Option<String>>("method")?.unwrap_or_else(|| "GET".to_owned());
        let body = ask.get::<Option<mlua::LuaString>>("body")?.map(|body| body.as_bytes().to_vec());
        let mut headers = Vec::new();
        if let Some(given) = ask.get::<Option<Table>>("headers")? {
            for pair in given.pairs::<String, String>() {
                headers.push(pair.map_err(|_| mlua::Error::runtime("each header needs a name and a text value"))?);
            }
        }
        let answer = lumen_win::http::request(&lumen_win::http::Request { url: &url, method: &method, headers: &headers, body: body.as_deref(), timeout_ms: NETWORK_TIMEOUT_MS, max_bytes: MAX_WEB_ANSWER }).map_err(mlua::Error::runtime)?;
        let result = lua.create_table()?;
        result.set("status", answer.status)?;
        result.set("body", lua.create_string(&answer.body)?)?;
        Ok(result)
    })?)?;

    // Programs.
    let run = permissions.run;
    lumen.set("run", lua.create_function(move |_, (program, arguments): (String, Option<Vec<String>>)| {
        if !run {
            return Err(refused("run programs"));
        }
        run_program(&program, &arguments.unwrap_or_default()).map_err(mlua::Error::runtime)
    })?)?;
    lumen.set("open", lua.create_function(move |_, target: String| {
        if !run {
            return Err(refused("open programs, files or web pages"));
        }
        Ok(lumen_win::shell::open(&target))
    })?)?;
    Ok(lumen)
}

/// How deep JSON may nest before it is refused.
const JSON_DEPTH: usize = 32;

fn from_json(lua: &Lua, value: &serde_json::Value, depth: usize) -> mlua::Result<Value> {
    if depth > JSON_DEPTH {
        return Err(mlua::Error::runtime("that JSON is nested too deeply"));
    }
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(flag) => Value::Boolean(*flag),
        serde_json::Value::Number(number) => Value::Number(number.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(text) => Value::String(lua.create_string(text)?),
        serde_json::Value::Array(items) => {
            let table = lua.create_table()?;
            for (index, item) in items.iter().enumerate() {
                table.set(index + 1, from_json(lua, item, depth + 1)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(fields) => {
            let table = lua.create_table()?;
            for (name, item) in fields {
                table.set(name.as_str(), from_json(lua, item, depth + 1)?)?;
            }
            Value::Table(table)
        }
    })
}

fn to_json(value: &Value, depth: usize) -> mlua::Result<serde_json::Value> {
    if depth > JSON_DEPTH {
        return Err(mlua::Error::runtime("that table is nested too deeply to write as JSON"));
    }
    Ok(match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(flag) => serde_json::Value::Bool(*flag),
        Value::Integer(number) => serde_json::json!(number),
        Value::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 { serde_json::json!(*number as i64) } else { serde_json::json!(number) }
        }
        Value::String(text) => serde_json::Value::String(text.to_string_lossy()),
        Value::Table(table) => {
            // A table numbered 1..n is a list; anything else is named fields.
            let length = table.raw_len();
            if length > 0 && table.pairs::<Value, Value>().count() == length {
                let mut items = Vec::with_capacity(length);
                for index in 1..=length {
                    items.push(to_json(&table.get::<Value>(index)?, depth + 1)?);
                }
                serde_json::Value::Array(items)
            } else {
                let mut fields = serde_json::Map::new();
                for pair in table.pairs::<Value, Value>() {
                    let (name, item) = pair?;
                    let name = match name {
                        Value::String(text) => text.to_string_lossy(),
                        Value::Integer(number) => number.to_string(),
                        Value::Number(number) => number.to_string(),
                        _ => continue,
                    };
                    fields.insert(name, to_json(&item, depth + 1)?);
                }
                serde_json::Value::Object(fields)
            }
        }
        _ => return Err(mlua::Error::runtime("only text, numbers, true/false and tables can be written as JSON")),
    })
}

/// Runs a program without a window, waits for it (for a limited time) and returns what it printed.
fn run_program(program: &str, arguments: &[String]) -> Result<String, String> {
    use std::{io::Read, os::windows::process::CommandExt, process::Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    // Read on another thread, so a program that prints a lot cannot block on a full pipe.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.take(MAX_OUTPUT as u64).read_to_end(&mut output);
        }
        output
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > PROGRAM_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} was still running after {} seconds and was stopped", PROGRAM_TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(format!("could not wait for {program}: {error}")),
        }
    }
    let output = reader.join().unwrap_or_default();
    Ok(String::from_utf8_lossy(&output).trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Permissions = Permissions { files: true, network: true, run: true };
    const NONE: Permissions = Permissions { files: false, network: false, run: false };

    fn live() -> Values {
        Values::from([("stats.cpu".to_owned(), "12%".to_owned())])
    }

    #[test]
    fn a_script_hands_values_to_its_widget() {
        let engine = Engine::new("t", r#"
            local count = 0
            function update()
                count += 1
                return { greeting = "Hello", count = count, load = lumen.value("stats.cpu"), half = 0.5, on = true, off = false, nothing = lumen.value("no.such") }
            end
        "#, NONE).unwrap();
        let first = engine.update(&live()).unwrap();
        assert_eq!((first["greeting"].as_str(), first["count"].as_str(), first["load"].as_str(), first["half"].as_str(), first["on"].as_str(), first["off"].as_str()), ("Hello", "1", "12%", "0.50", "yes", ""));
        assert!(!first.contains_key("nothing"));
        assert_eq!(engine.update(&live()).unwrap()["count"], "2", "the script keeps its own state between runs");
    }

    #[test]
    fn a_script_without_update_or_click_is_fine() {
        let engine = Engine::new("t", "local x = 1", NONE).unwrap();
        assert!(engine.update(&live()).unwrap().is_empty() && engine.click("a", &live()).unwrap().is_empty());
        let clicker = Engine::new("t", "function click(part) return { last = part } end", NONE).unwrap();
        assert_eq!(clicker.click("star", &live()).unwrap()["last"], "star");
    }

    #[test]
    fn mistakes_are_reported_in_plain_words() {
        assert!(Engine::new("t", "function update( return 1", NONE).is_err(), "a script that does not read");
        let engine = Engine::new("t", "function update() error('the file is missing') end", NONE).unwrap();
        assert!(engine.update(&live()).unwrap_err().contains("the file is missing"));
        let odd = Engine::new("t", "function update() return 'not a table' end", NONE).unwrap();
        assert!(odd.update(&live()).unwrap().is_empty());
    }

    #[test]
    fn nothing_powerful_works_without_permission() {
        let engine = Engine::new("t", r#"
            function click(what)
                if what == "read" then lumen.read_file("C:\\Windows\\win.ini")
                elseif what == "write" then lumen.write_file("C:\\nope.txt", "x")
                elseif what == "list" then lumen.list_folder("C:\\")
                elseif what == "net" then lumen.http_get("https://example.com")
                elseif what == "post" then lumen.http_request({ url = "http://homeassistant.local:8123/api/services/light/toggle", method = "POST" })
                elseif what == "run" then lumen.run("cmd", { "/c", "echo hi" })
                else lumen.open("notepad.exe") end
            end
        "#, NONE).unwrap();
        for what in ["read", "write", "list", "net", "post", "run", "open"] {
            let error = engine.click(what, &live()).unwrap_err();
            assert!(error.contains("has not been allowed to"), "{what}: {error}");
        }
        assert!(!std::path::Path::new("C:\\nope.txt").exists());
    }

    #[test]
    fn with_permission_a_script_can_use_files_and_run_programs() {
        let dir = std::env::temp_dir().join(format!("lumen-script-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.txt").to_string_lossy().replace('\\', "/");
        let source = format!(r#"
            function update()
                lumen.write_file("{file}", "written by a script")
                local listed = lumen.list_folder("{}")
                return {{ text = lumen.read_file("{file}"), files = #listed, said = lumen.run("cmd", {{ "/c", "echo hello from cmd" }}) }}
            end
        "#, dir.to_string_lossy().replace('\\', "/"));
        let engine = Engine::new("t", &source, ALL).unwrap();
        let values = engine.update(&live()).unwrap();
        assert_eq!((values["text"].as_str(), values["files"].as_str(), values["said"].as_str()), ("written by a script", "1", "hello from cmd"));
        let _ = std::fs::remove_dir_all(dir);

        let web = Engine::new("t", r#"function update() return { page = lumen.http_get("ftp://example.com") } end"#, ALL).unwrap();
        assert!(web.update(&live()).unwrap_err().contains("must start with https:// or http://"));
    }

    #[test]
    fn a_script_stuck_in_a_loop_is_stopped() {
        let started = Instant::now();
        assert!(Engine::new("t", "while true do end", NONE).err().unwrap().contains("ran for too long"), "even at load");
        let engine = Engine::new("t", "function update() while true do end end", NONE).unwrap();
        assert!(engine.update(&live()).unwrap_err().contains("ran for too long"));
        assert!(started.elapsed() < RUN_LIMIT * 4);
        // And it can be called again afterwards.
        assert!(engine.click("x", &live()).unwrap().is_empty());
    }

    #[test]
    fn a_script_cannot_take_all_the_memory_or_reach_outside_its_sandbox() {
        let hungry = Engine::new("t", "function update() local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1000) end end", NONE).unwrap();
        assert!(hungry.update(&live()).is_err(), "stopped by the memory or the time limit");
        let engine = Engine::new("t", "function update() return { io = type(io), osexit = type(os.exit), loader = type(loadstring), require = type(require) } end", NONE).unwrap();
        let seen = engine.update(&live()).unwrap();
        assert_eq!((seen["io"].as_str(), seen["osexit"].as_str()), ("nil", "nil"), "no raw file or process access outside the lumen library");
    }

    #[test]
    fn json_goes_both_ways_without_any_permission() {
        let engine = Engine::new("t", r#"
            function update()
                local state = lumen.json_decode('{"entity_id":"light.desk","state":"on","attributes":{"brightness":128,"names":["a","b"]},"gone":null}')
                local sent = lumen.json_encode({ entity_id = state.entity_id, level = 0.5, on = true, list = { 1, 2, 3 } })
                local back = lumen.json_decode(sent)
                return { state = state.state, brightness = state.attributes.brightness, second = state.attributes.names[2], gone = tostring(state.gone),
                         entity = back.entity_id, level = back.level, third = back.list[3], bad = tostring(pcall(lumen.json_decode, "{nope")) }
            end
        "#, NONE).unwrap();
        let seen = engine.update(&live()).unwrap();
        assert_eq!((seen["state"].as_str(), seen["brightness"].as_str(), seen["second"].as_str(), seen["gone"].as_str()), ("on", "128", "b", "nil"));
        assert_eq!((seen["entity"].as_str(), seen["level"].as_str(), seen["third"].as_str(), seen["bad"].as_str()), ("light.desk", "0.50", "3", "false"));
    }

}
