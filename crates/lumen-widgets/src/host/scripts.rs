//! Scripts in the host: which widget files name a script, whether the user
//! has agreed to it, and running the approved ones.
//!
//! A script never runs until the user has agreed to its exact text and to
//! what its widget file says it needs. Change either and it stops until they
//! agree again. Each approved script has a thread and a Luau machine of its
//! own, so nothing a script does (a slow download, a long sum) can hold up
//! drawing. Scripts are only run while a widget that uses them is on screen
//! and no fullscreen app or game is in front.

use std::sync::mpsc;

use lumen_ipc::{
    scripts::ScriptInfo,
    widget_file::{self, Permissions},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;
use crate::script::{Engine, Values};

pub(super) const TIMER_SCRIPTS: usize = 9;
/// A script that keeps failing is tried less and less often, up to this.
const RETRY_LONGEST: Duration = Duration::from_secs(300);

/// What the user agreed to for one widget file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Approval {
    hash: String,
    permissions: Permissions,
}

enum Job {
    Update(Values),
    Click(String, Values),
}

/// What a script's thread hands back after each call.
pub(super) struct ScriptResult {
    file: String,
    generation: u64,
    click: bool,
    /// Processor time the call used (not time spent waiting on a download or a program).
    took: Duration,
    result: Result<Values, String>,
}

struct Runner {
    jobs: mpsc::Sender<Job>,
    /// Tells this runner's results from those of one it replaced.
    generation: u64,
    hash: String,
    permissions: Permissions,
    every: Duration,
    /// An update is on its way and has not come back.
    busy: bool,
    due: Instant,
    failures: u32,
}

#[derive(Default)]
pub(super) struct Scripts {
    pub(super) list: Vec<ScriptInfo>,
    approvals: HashMap<String, Approval>,
    runners: HashMap<String, Runner>,
    /// What each script last handed back, by widget file.
    values: HashMap<String, Values>,
    generation: u64,
    nothing: Values,
}

impl Scripts {
    /// What a widget drawn from `file` should show of its script: the values
    /// (`None` if the file has no script), and a line to show if the script is not running.
    pub(super) fn view(&self, file: Option<&str>) -> (Option<&Values>, Option<String>) {
        let Some(info) = file.and_then(|file| self.list.iter().find(|info| info.file == file)) else { return (None, None) };
        let notice = if info.missing {
            Some(format!("{} is missing", info.script))
        } else if !info.approved {
            Some("Script waiting for your OK in Lumen".to_owned())
        } else {
            info.error.as_ref().map(|error| format!("Script: {error}"))
        };
        (Some(self.values.get(&info.file).unwrap_or(&self.nothing)), notice)
    }
}

/// A fingerprint of a script's text (SHA-256, in hex).
pub(super) fn fingerprint(text: &str) -> String {
    Sha256::digest(text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_script(folder: &std::path::Path, name: &str) -> Option<String> {
    let path = folder.join(name);
    (widget_file::is_script_name(name) && std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() <= widget_file::MAX_SCRIPT_BYTES)).then(|| std::fs::read_to_string(&path).ok()).flatten()
}

/// How long to leave a script alone after this many failures in a row.
fn retry_delay(every: Duration, failures: u32) -> Duration {
    (every * 2u32.saturating_pow(failures.min(10))).min(RETRY_LONGEST).max(every)
}

/// Processor time this thread has used so far.
fn thread_cpu_time() -> Duration {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentThread, GetThreadTimes},
    };
    let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetThreadTimes(GetCurrentThread(), &mut created, &mut exited, &mut kernel, &mut user) }.is_err() {
        return Duration::ZERO;
    }
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

impl Host {
    pub(super) fn load_scripts(&mut self) {
        let text = std::fs::read_to_string(&self.options.script_approvals_path).ok();
        self.scripts.approvals = text.and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
        self.sync_scripts();
    }

    fn save_approvals(&self) {
        match serde_json::to_string_pretty(&self.scripts.approvals) {
            Ok(text) => {
                if let Err(error) = std::fs::write(&self.options.script_approvals_path, text) {
                    warn!("could not save {}: {error}", self.options.script_approvals_path.display());
                }
            }
            Err(error) => warn!("{error}"),
        }
    }

    /// Works out afresh which scripts exist and which may run, starting and
    /// stopping their threads to match. Returns whether anything changed.
    pub(super) fn sync_scripts(&mut self) -> bool {
        let folder = self.options.widget_files_dir.clone();
        let mut list = Vec::new();
        let mut to_start = Vec::new();
        for file in self.files.list.iter().filter(|file| file.is_widget) {
            let Some(script) = &file.script else { continue };
            let text = read_script(&folder, script);
            let hash = text.as_deref().map(fingerprint).unwrap_or_default();
            let agreed = self.scripts.approvals.get(&file.file);
            let approved = text.is_some() && agreed.is_some_and(|agreed| agreed.hash == hash && agreed.permissions == file.permissions);
            // What went wrong last time still stands if the script is the same one.
            let error = self.scripts.list.iter().find(|old| old.file == file.file && old.hash == hash && approved).and_then(|old| old.error.clone());
            list.push(ScriptInfo { file: file.file.clone(), name: file.name.clone(), script: script.clone(), permissions: file.permissions, hash: hash.clone(), approved, changed: agreed.is_some() && !approved && text.is_some(), missing: text.is_none(), error });
            let every = Duration::from_secs(u64::from(file.every.max(widget_file::EVERY_RANGE.0)));
            match self.scripts.runners.get_mut(&file.file) {
                Some(runner) if approved && runner.hash == hash && runner.permissions == file.permissions => runner.every = every,
                _ if approved => to_start.push((file.file.clone(), script.clone(), text.unwrap_or_default(), hash, file.permissions, every)),
                _ => {}
            }
        }
        // A script that is gone or no longer approved stops: dropping its runner ends its thread.
        let allowed = |file: &str| list.iter().any(|info| info.file == file && info.approved);
        self.scripts.runners.retain(|file, _| allowed(file));
        self.scripts.values.retain(|file, _| allowed(file));
        for (file, script, source, hash, permissions, every) in to_start {
            self.start_script(file, script, source, hash, permissions, every);
        }
        let changed = list != self.scripts.list;
        if changed {
            self.scripts.list = list;
            (self.on_state)(Report::Scripts(self.scripts.list.clone()));
        }
        changed
    }

    fn start_script(&mut self, file: String, script: String, source: String, hash: String, permissions: Permissions, every: Duration) {
        self.scripts.generation += 1;
        let generation = self.scripts.generation;
        let (jobs, waiting) = mpsc::channel::<Job>();
        let handle = HostHandle { window: self.window.0 as isize, inbox: self.inbox.clone() };
        let name = file.clone();
        let spawned = std::thread::Builder::new().name(format!("script {script}")).spawn(move || {
            let report = |click: bool, took: Duration, result: Result<Values, String>| handle.script_result(ScriptResult { file: name.clone(), generation, click, took, result });
            let before = thread_cpu_time();
            let engine = match Engine::new(&script, &source, permissions) {
                Ok(engine) => engine,
                Err(error) => return report(false, thread_cpu_time().saturating_sub(before), Err(error)),
            };
            while let Ok(job) = waiting.recv() {
                let before = thread_cpu_time();
                let (click, result) = match job {
                    Job::Update(live) => (false, engine.update(&live)),
                    Job::Click(part, live) => (true, engine.click(&part, &live)),
                };
                report(click, thread_cpu_time().saturating_sub(before), result);
            }
        });
        match spawned {
            Ok(_) => {
                info!("script started for {file}");
                self.scripts.values.remove(&file);
                self.scripts.runners.insert(file, Runner { jobs, generation, hash, permissions, every, busy: false, due: Instant::now(), failures: 0 });
            }
            Err(error) => warn!("could not start the script for {file}: {error}"),
        }
    }

    /// The user agreed to a script as it is now (`hash` is the text they were shown).
    pub(super) fn approve_script(&mut self, file: &str, hash: &str) {
        let Some(info) = self.scripts.list.iter().find(|info| info.file == file && info.hash == hash && !info.missing) else {
            warn!("{file}: its script changed before it was approved; nothing was allowed");
            return;
        };
        self.scripts.approvals.insert(file.to_owned(), Approval { hash: hash.to_owned(), permissions: info.permissions });
        self.save_approvals();
        self.scripts_settled();
    }

    pub(super) fn revoke_script(&mut self, file: &str) {
        if self.scripts.approvals.remove(file).is_some() {
            self.save_approvals();
            self.scripts_settled();
        }
    }

    fn scripts_settled(&mut self) {
        if self.sync_scripts() {
            self.draw(None);
        }
        self.sync_timers();
    }

    /// Whether a widget drawn from this file is on screen.
    fn script_on_screen(&self, file: &str) -> bool {
        self.slots.iter().any(|slot| slot.surface.is_visible() && self.placement(&slot.id).is_some_and(|placement| placement.file.as_deref() == Some(file)))
    }

    /// How long until a script is next due, or `None` if none should run now.
    pub(super) fn scripts_due_in(&self) -> Option<u32> {
        if self.paused || self.gpu.is_none() {
            return None;
        }
        let now = Instant::now();
        let soonest = self.scripts.runners.iter().filter(|(file, runner)| !runner.busy && self.script_on_screen(file)).map(|(_, runner)| runner.due.saturating_duration_since(now)).min()?;
        Some((soonest.as_millis().min(3_600_000) as u32).max(30))
    }

    /// Every live value Lumen has, for a script to ask for by name.
    fn live_values(&self) -> Values {
        let data = self.part_data(false);
        parts::SOURCES.iter().copied().chain(crate::weather::SOURCES.iter().map(|(name, _)| *name)).filter_map(|name| parts::value_of(name, &data).map(|value| (name.to_owned(), value))).collect()
    }

    /// Handles the scripts timer; `false` if the timer is not this one.
    pub(super) fn on_scripts_timer(&mut self, timer: usize) -> bool {
        if timer != TIMER_SCRIPTS {
            return false;
        }
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_SCRIPTS);
        }
        if !self.paused && self.gpu.is_some() {
            let now = Instant::now();
            let due: Vec<String> = self.scripts.runners.iter().filter(|(file, runner)| !runner.busy && runner.due <= now && self.script_on_screen(file)).map(|(file, _)| file.clone()).collect();
            if !due.is_empty() {
                let live = self.live_values();
                for file in due {
                    self.send_update(&file, live.clone());
                }
            }
        }
        self.sync_timers();
        true
    }

    fn send_update(&mut self, file: &str, live: Values) {
        let Some(runner) = self.scripts.runners.get_mut(file) else { return };
        if runner.jobs.send(Job::Update(live)).is_ok() {
            runner.busy = true;
        } else {
            // Its thread has ended (the script could not be read): leave it be.
            runner.due = Instant::now() + RETRY_LONGEST;
        }
    }

    /// A part that says `click: script` was clicked: tell the script, then
    /// ask it for its values again so the widget shows what the click did.
    pub(super) fn script_clicked(&mut self, file: &str, part: String) {
        if self.paused || !self.scripts.runners.contains_key(file) {
            return;
        }
        let live = self.live_values();
        if let Some(runner) = self.scripts.runners.get(file) {
            let _ = runner.jobs.send(Job::Click(part, live.clone()));
        }
        self.send_update(file, live);
        self.sync_timers();
    }

    /// A script's call finished (on its own thread; this runs on the host's).
    pub(super) fn on_script_result(&mut self, result: ScriptResult) {
        let ScriptResult { file, generation, click, took, result } = result;
        let Some(runner) = self.scripts.runners.get_mut(&file).filter(|runner| runner.generation == generation) else { return };
        let now = Instant::now();
        if !click {
            runner.busy = false;
        }
        let (mut redraw, error) = (false, result.as_ref().err().cloned());
        match result {
            Ok(values) => {
                let held = self.scripts.values.entry(file.clone()).or_default();
                if click {
                    // What a click hands back is added to what is showing.
                    redraw = !values.is_empty();
                    held.extend(values);
                } else {
                    runner.failures = 0;
                    runner.due = now + runner.every;
                    redraw = *held != values;
                    *held = values;
                }
            }
            Err(ref message) => {
                runner.failures += 1;
                runner.due = now + retry_delay(runner.every, runner.failures);
                info!("script for {file}: {message}");
            }
        }
        // The script's work is work done for the widgets that show it.
        let showing: Vec<String> = self.layout.widgets.iter().filter(|widget| widget.file.as_deref() == Some(file.as_str())).map(|widget| widget.id.clone()).collect();
        for id in &showing {
            self.governor.record_work(id, took / showing.len().max(1) as u32);
        }
        // A click that worked says nothing about whether updates do.
        if let Some(info) = self.scripts.list.iter_mut().find(|info| info.file == file).filter(|info| info.error != error && !(click && error.is_none())) {
            info.error = error;
            redraw = true;
            (self.on_state)(Report::Scripts(self.scripts.list.clone()));
        }
        if redraw {
            self.draw(None);
        }
        self.sync_timers();
    }

    /// Stops every script (the host is closing).
    pub(super) fn stop_scripts(&mut self) {
        self.scripts.runners.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_follows_every_character_of_the_text() {
        let one = fingerprint("function update() end");
        assert_eq!(one.len(), 64);
        assert_eq!(one, fingerprint("function update() end"));
        assert_ne!(one, fingerprint("function update() end "));
    }

    #[test]
    fn a_failing_script_is_tried_less_and_less_often() {
        let every = Duration::from_secs(5);
        let secs: Vec<u64> = (1..=8).map(|failures| retry_delay(every, failures).as_secs()).collect();
        assert_eq!(secs, [10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(retry_delay(Duration::from_secs(900), 3).as_secs(), 900, "never sooner than its own interval");
        assert_eq!(retry_delay(every, u32::MAX).as_secs(), 300);
    }

    #[test]
    fn a_widget_without_approval_shows_a_notice_and_no_values() {
        let info = |approved: bool, missing: bool, error: Option<&str>| ScriptInfo { file: "w.yaml".into(), name: "W".into(), script: "w.luau".into(), permissions: Permissions::default(), hash: "h".into(), approved, changed: false, missing, error: error.map(str::to_owned) };
        let mut scripts = Scripts::default();
        assert_eq!(scripts.view(Some("w.yaml")), (None, None), "a file with no script");
        scripts.list = vec![info(false, false, None)];
        let (values, notice) = scripts.view(Some("w.yaml"));
        assert!(values.is_some_and(Values::is_empty) && notice.unwrap().contains("waiting for your OK"));
        scripts.list = vec![info(false, true, None)];
        assert!(scripts.view(Some("w.yaml")).1.unwrap().contains("w.luau is missing"));
        scripts.list = vec![info(true, false, Some("it broke"))];
        assert_eq!(scripts.view(Some("w.yaml")).1.as_deref(), Some("Script: it broke"));
        scripts.list = vec![info(true, false, None)];
        scripts.values.insert("w.yaml".into(), Values::from([("a".to_owned(), "1".to_owned())]));
        let (values, notice) = scripts.view(Some("w.yaml"));
        assert_eq!((values.unwrap()["a"].as_str(), notice), ("1", None));
        assert_eq!(scripts.view(None), (None, None));
    }
}
