//! The performance governor: measures what each widget costs and steps in
//! when one costs too much.
//!
//! Every time a widget is drawn (or work is done on its behalf, such as
//! sampling system stats), the time spent is recorded against it. At the end
//! of each measuring window the governor works out the widget's share of the
//! whole CPU. Over budget is a warning; far over budget for several windows
//! in a row switches the widget off until the user resumes it.
//!
//! The built-in widgets sit far below their budgets. The governor exists so
//! that a badly written third-party widget cannot make Lumen heavy.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use lumen_ipc::widgets::{CostStatus, WIDGET_CPU_BUDGET, WIDGET_MEMORY_BUDGET_MB, WidgetCost};

/// How long each measurement covers.
pub const WINDOW: Duration = Duration::from_secs(10);
/// A widget this many times over its CPU budget...
const SUSPEND_FACTOR: f64 = 10.0;
/// ...for this many windows in a row is switched off.
const SUSPEND_AFTER_WINDOWS: u8 = 3;

#[derive(Default)]
struct Entry {
    /// Time spent on this widget in the current window.
    busy: Duration,
    draws: u32,
    /// Results of the last finished window.
    cpu_percent: f64,
    draws_per_minute: f64,
    /// Consecutive windows far over budget.
    strikes: u8,
    suspended: bool,
}

pub struct Governor {
    cores: f64,
    window_started: Instant,
    entries: HashMap<String, Entry>,
}

impl Governor {
    pub fn new(now: Instant) -> Self {
        let cores = std::thread::available_parallelism().map_or(1.0, |n| n.get() as f64);
        Self::with_cores(cores, now)
    }

    fn with_cores(cores: f64, now: Instant) -> Self {
        Self { cores, window_started: now, entries: HashMap::new() }
    }

    /// Records time spent drawing a widget.
    pub fn record_draw(&mut self, id: &str, spent: Duration) {
        let entry = self.entries.entry(id.to_owned()).or_default();
        entry.busy += spent;
        entry.draws += 1;
    }

    /// Records other work done for a widget (fetching its data).
    pub fn record_work(&mut self, id: &str, spent: Duration) {
        self.entries.entry(id.to_owned()).or_default().busy += spent;
    }

    /// Closes the measuring window if it has run its length. Returns the ids
    /// of widgets that have just been switched off.
    pub fn roll(&mut self, now: Instant) -> Vec<String> {
        let elapsed = now.saturating_duration_since(self.window_started);
        if elapsed < WINDOW {
            return Vec::new();
        }
        self.window_started = now;
        let mut newly_suspended = Vec::new();
        for (id, entry) in &mut self.entries {
            entry.cpu_percent = entry.busy.as_secs_f64() / elapsed.as_secs_f64() / self.cores * 100.0;
            entry.draws_per_minute = f64::from(entry.draws) * 60.0 / elapsed.as_secs_f64();
            entry.busy = Duration::ZERO;
            entry.draws = 0;
            if entry.cpu_percent > WIDGET_CPU_BUDGET * SUSPEND_FACTOR {
                entry.strikes = entry.strikes.saturating_add(1);
                if entry.strikes >= SUSPEND_AFTER_WINDOWS && !entry.suspended {
                    entry.suspended = true;
                    newly_suspended.push(id.clone());
                }
            } else {
                entry.strikes = 0;
            }
        }
        newly_suspended.sort();
        newly_suspended
    }

    pub fn is_suspended(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.suspended)
    }

    /// Ids of every widget that is switched off, in order.
    pub fn suspended(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.entries.iter().filter(|(_, entry)| entry.suspended).map(|(id, _)| id.clone()).collect();
        ids.sort();
        ids
    }

    /// Lets a switched-off widget run again, with a clean record.
    pub fn resume(&mut self, id: &str) -> bool {
        match self.entries.get_mut(id) {
            Some(entry) if entry.suspended => {
                *entry = Entry::default();
                true
            }
            _ => false,
        }
    }

    /// Stops tracking a widget that has been removed.
    pub fn forget(&mut self, id: &str) {
        self.entries.remove(id);
    }

    /// A widget's cost as of the last finished window. `memory_mb` is supplied
    /// by the caller, who knows what the widget holds.
    pub fn cost(&self, id: &str, memory_mb: f64) -> WidgetCost {
        let entry = self.entries.get(id);
        let cpu_percent = entry.map_or(0.0, |e| e.cpu_percent);
        let status = if entry.is_some_and(|e| e.suspended) {
            CostStatus::Suspended
        } else if cpu_percent > WIDGET_CPU_BUDGET || memory_mb > WIDGET_MEMORY_BUDGET_MB {
            CostStatus::OverBudget
        } else {
            CostStatus::Ok
        };
        WidgetCost { id: id.to_owned(), cpu_percent, memory_mb, draws_per_minute: entry.map_or(0.0, |e| e.draws_per_minute), status }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A governor on a pretend 10-core machine, so 1 s of work in a 10 s window is 1% of the CPU.
    fn governor() -> (Governor, Instant) {
        let start = Instant::now();
        (Governor::with_cores(10.0, start), start)
    }

    #[test]
    fn a_light_widget_is_within_budget() {
        let (mut governor, start) = governor();
        // A clock: one 2 ms redraw in the window.
        governor.record_draw("clock-1", Duration::from_millis(2));
        assert!(governor.roll(start + Duration::from_secs(5)).is_empty(), "the window is not over yet");
        assert_eq!(governor.cost("clock-1", 0.4).cpu_percent, 0.0, "nothing measured until a window ends");

        assert!(governor.roll(start + WINDOW).is_empty());
        let cost = governor.cost("clock-1", 0.4);
        assert!((cost.cpu_percent - 0.002).abs() < 1e-9, "{}", cost.cpu_percent);
        assert!((cost.draws_per_minute - 6.0).abs() < 1e-9);
        assert_eq!(cost.status, CostStatus::Ok);
    }

    #[test]
    fn over_budget_is_a_warning_not_a_switch_off() {
        let (mut governor, start) = governor();
        // 200 ms of work per 10 s window on 10 cores = 0.2% of the CPU: twice the budget.
        for window in 1..=5 {
            governor.record_draw("heavy", Duration::from_millis(200));
            assert!(governor.roll(start + WINDOW * window).is_empty());
        }
        let cost = governor.cost("heavy", 1.0);
        assert_eq!(cost.status, CostStatus::OverBudget);
        assert!(!governor.is_suspended("heavy"));
    }

    #[test]
    fn too_much_memory_is_also_a_warning() {
        let (governor, _) = governor();
        assert_eq!(governor.cost("big", WIDGET_MEMORY_BUDGET_MB + 1.0).status, CostStatus::OverBudget);
        assert_eq!(governor.cost("small", 0.5).status, CostStatus::Ok);
    }

    #[test]
    fn far_over_budget_for_three_windows_switches_it_off() {
        let (mut governor, start) = governor();
        // 2 s of work per 10 s window on 10 cores = 2% of the CPU: twenty times the budget.
        governor.record_work("runaway", Duration::from_secs(2));
        assert!(governor.roll(start + WINDOW).is_empty());
        governor.record_work("runaway", Duration::from_secs(2));
        assert!(governor.roll(start + WINDOW * 2).is_empty());
        governor.record_work("runaway", Duration::from_secs(2));
        assert_eq!(governor.roll(start + WINDOW * 3), ["runaway"]);

        assert!(governor.is_suspended("runaway"));
        assert_eq!(governor.suspended(), ["runaway"]);
        assert_eq!(governor.cost("runaway", 1.0).status, CostStatus::Suspended);
        // It is reported once, not every window.
        assert!(governor.roll(start + WINDOW * 4).is_empty());
    }

    #[test]
    fn one_quiet_window_clears_the_strikes() {
        let (mut governor, start) = governor();
        governor.record_work("spiky", Duration::from_secs(2));
        governor.roll(start + WINDOW);
        governor.record_work("spiky", Duration::from_secs(2));
        governor.roll(start + WINDOW * 2);
        // A calm window in between: a brief spike is not a runaway widget.
        governor.roll(start + WINDOW * 3);
        governor.record_work("spiky", Duration::from_secs(2));
        assert!(governor.roll(start + WINDOW * 4).is_empty());
        assert!(!governor.is_suspended("spiky"));
    }

    #[test]
    fn resume_gives_a_clean_start_and_forget_drops_the_record() {
        let (mut governor, start) = governor();
        for window in 1..=3 {
            governor.record_work("runaway", Duration::from_secs(2));
            governor.roll(start + WINDOW * window);
        }
        assert!(governor.resume("runaway"));
        assert!(!governor.is_suspended("runaway"));
        assert_eq!(governor.cost("runaway", 1.0).cpu_percent, 0.0);
        assert!(!governor.resume("runaway"), "nothing to resume");
        assert!(!governor.resume("never-seen"));

        governor.record_draw("gone", Duration::from_millis(1));
        governor.forget("gone");
        governor.roll(start + WINDOW * 4);
        assert_eq!(governor.cost("gone", 0.0).draws_per_minute, 0.0);
    }

    #[test]
    fn a_long_gap_is_measured_over_its_real_length() {
        let (mut governor, start) = governor();
        // 1 s of work, but the window was closed after 100 s (the PC was asleep, or a game was running).
        governor.record_work("w", Duration::from_secs(1));
        governor.roll(start + Duration::from_secs(100));
        assert!((governor.cost("w", 0.0).cpu_percent - 0.1).abs() < 1e-9);
    }
}
