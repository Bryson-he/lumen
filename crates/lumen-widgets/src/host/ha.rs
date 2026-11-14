//! Home Assistant in the host: the saved connection, the entities widgets
//! are showing, and when they are read again.
//!
//! Lumen asks Home Assistant only for entities a widget on screen mentions,
//! every few seconds, and never while a fullscreen app or game is in front.
//! If Home Assistant refuses the token, Lumen stops asking until the token
//! is changed: Home Assistant logs every refused try, and can lock a PC out
//! after too many.

use std::collections::BTreeSet;

use lumen_ipc::home_assistant::{HaConfig, HaState, HaStatus, HaValue, entities_in};

use super::*;
use crate::home_assistant::{self as api, HaError};

pub(super) const TIMER_HA: usize = 10;
/// How often the entities on screen are read.
const REFRESH: Duration = Duration::from_secs(10);
/// After a click, read again this soon, so the widget shows what happened.
const AFTER_CLICK: Duration = Duration::from_millis(700);
const RETRY_LONGEST: Duration = Duration::from_secs(300);

/// What a worker thread hands back.
pub(super) enum HaResult {
    /// The connection was tried (after the settings changed).
    Checked(HaConfig, Result<String, HaError>),
    /// The entities on screen were read.
    Read(HaConfig, Result<HashMap<String, HaValue>, HaError>),
    /// A click was carried out.
    Clicked(Result<(), HaError>),
}

#[derive(Default)]
pub(super) struct Ha {
    config: HaConfig,
    pub(super) state: HaState,
    pub(super) values: HashMap<String, HaValue>,
    busy: bool,
    due: Option<Instant>,
    failures: u32,
}

impl Ha {
    /// The values to draw with: `None` until a connection is set, so `{ha.…}` then shows nothing at all.
    pub(super) fn view(&self) -> Option<&HashMap<String, HaValue>> {
        self.config.is_set().then_some(&self.values)
    }
}

impl Host {
    pub(super) fn load_ha(&mut self) {
        let text = std::fs::read_to_string(&self.options.home_assistant_path).ok();
        let config = text.and_then(|text| serde_json::from_str::<HaConfig>(text.trim_start_matches('\u{feff}')).ok()).map(HaConfig::sanitised).unwrap_or_default();
        self.ha.config = config;
        self.ha_settled(if self.ha.config.is_set() { HaStatus::Checking } else { HaStatus::NotSet }, None, None);
        if self.ha.config.is_set() {
            self.start_ha_check();
        }
    }

    /// New connection settings: save them, forget what was read, and try them.
    pub(super) fn set_ha(&mut self, config: HaConfig) {
        let config = config.sanitised();
        match serde_json::to_string_pretty(&config) {
            Ok(text) => {
                if let Err(error) = std::fs::write(&self.options.home_assistant_path, text) {
                    warn!("could not save {}: {error}", self.options.home_assistant_path.display());
                }
            }
            Err(error) => warn!("{error}"),
        }
        self.ha.config = config;
        self.ha.values.clear();
        self.ha.failures = 0;
        self.ha.due = None;
        self.ha_settled(if self.ha.config.is_set() { HaStatus::Checking } else { HaStatus::NotSet }, None, None);
        if self.ha.config.is_set() {
            self.start_ha_check();
        }
        self.draw(None);
        self.sync_timers();
    }

    fn ha_settled(&mut self, status: HaStatus, name: Option<String>, error: Option<String>) {
        let state = HaState { address: self.ha.config.address.clone(), has_token: !self.ha.config.token.is_empty(), status, name: name.or_else(|| self.ha.state.name.clone().filter(|_| status == HaStatus::Connected)), error };
        if state != self.ha.state {
            self.ha.state = state.clone();
            (self.on_state)(Report::HomeAssistant(state));
        }
    }

    fn ha_worker(&mut self, work: impl FnOnce(HaConfig) -> HaResult + Send + 'static) {
        let config = self.ha.config.clone();
        let handle = HostHandle { window: self.window.0 as isize, inbox: self.inbox.clone() };
        let _ = std::thread::Builder::new().name("home-assistant".into()).spawn(move || handle.ha_result(work(config)));
    }

    fn start_ha_check(&mut self) {
        self.ha_worker(|config| {
            let result = api::check(&config);
            HaResult::Checked(config, result)
        });
    }

    /// The entities that widgets on screen mention.
    fn ha_entities_on_screen(&self) -> Vec<String> {
        let mut found = BTreeSet::new();
        for slot in self.slots.iter().filter(|slot| slot.surface.is_visible()) {
            let Some(placement) = self.placement(&slot.id) else { continue };
            for part in self.parts_of(placement).0.iter().filter(|part| !part.hidden) {
                for text in [&part.text, &part.source, &part.when, &part.alert_when, &part.click] {
                    entities_in(text, &mut found);
                }
            }
        }
        found.into_iter().take(64).collect()
    }

    /// How long until the entities should be read again, or `None` if they should not be.
    pub(super) fn ha_due_in(&self) -> Option<u32> {
        let ha = &self.ha;
        // A refused token is not tried again until it is changed.
        if !ha.config.is_set() || ha.busy || self.paused || self.gpu.is_none() || matches!(ha.state.status, HaStatus::Refused | HaStatus::Checking) {
            return None;
        }
        if self.ha_entities_on_screen().is_empty() {
            return None;
        }
        let wait = ha.due.map_or(Duration::ZERO, |due| due.saturating_duration_since(Instant::now()));
        Some((wait.as_millis() as u32).max(50))
    }

    /// Handles the Home Assistant timer; `false` if the timer is not this one.
    pub(super) fn on_ha_timer(&mut self, timer: usize) -> bool {
        if timer != TIMER_HA {
            return false;
        }
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_HA);
        }
        if self.ha_due_in().is_some() {
            let entities = self.ha_entities_on_screen();
            self.ha.busy = true;
            self.ha_worker(move |config| {
                let result = api::read(&config, &entities);
                HaResult::Read(config, result)
            });
        }
        true
    }

    /// A part that says `click: ha.toggle <entity>` was clicked.
    pub(super) fn ha_clicked(&mut self, entity: String) {
        if !self.ha.config.is_set() || self.ha.state.status == HaStatus::Refused {
            return;
        }
        let state = self.ha.values.get(&entity).map(|value| value.state.clone()).unwrap_or_default();
        // Show the likely result at once; the next read corrects it if Home Assistant disagrees.
        if let Some(value) = self.ha.values.get_mut(&entity) {
            match value.state.as_str() {
                "on" => value.state = "off".to_owned(),
                "off" => value.state = "on".to_owned(),
                _ => {}
            }
            self.draw(None);
        }
        self.ha_worker(move |config| HaResult::Clicked(api::click(&config, &entity, &state)));
    }

    /// A worker finished (on its own thread; this runs on the host's).
    pub(super) fn on_ha_result(&mut self, result: HaResult) {
        match result {
            HaResult::Checked(config, outcome) => {
                // The settings changed while it was away: this answer is for something else.
                if config != self.ha.config {
                    return;
                }
                match outcome {
                    Ok(name) => self.ha_settled(HaStatus::Connected, Some(name), None),
                    Err(HaError::Refused) => self.ha_settled(HaStatus::Refused, None, Some(HaError::Refused.message())),
                    Err(error) => {
                        // Not reachable right now; reading the entities will keep trying, less and less often.
                        self.ha.failures = 1;
                        self.ha.due = Some(Instant::now() + REFRESH * 3);
                        self.ha_settled(HaStatus::Unreachable, None, Some(error.message()));
                    }
                }
            }
            HaResult::Read(config, outcome) => {
                self.ha.busy = false;
                if config != self.ha.config {
                    self.sync_timers();
                    return;
                }
                match outcome {
                    Ok(values) => {
                        self.ha.failures = 0;
                        self.ha.due = Some(Instant::now() + REFRESH);
                        let changed = values != self.ha.values;
                        self.ha.values = values;
                        self.ha_settled(HaStatus::Connected, None, None);
                        if changed {
                            self.draw(None);
                        }
                    }
                    Err(HaError::Refused) => {
                        self.ha.values.clear();
                        self.ha_settled(HaStatus::Refused, None, Some(HaError::Refused.message()));
                        self.draw(None);
                    }
                    Err(error) => {
                        self.ha.failures += 1;
                        self.ha.due = Some(Instant::now() + (REFRESH * 2u32.saturating_pow(self.ha.failures.min(8))).min(RETRY_LONGEST));
                        self.ha_settled(HaStatus::Unreachable, None, Some(error.message()));
                    }
                }
            }
            HaResult::Clicked(outcome) => {
                match outcome {
                    Ok(()) => {}
                    Err(HaError::Refused) => self.ha_settled(HaStatus::Refused, None, Some(HaError::Refused.message())),
                    Err(error) => info!("home assistant: {}", error.message()),
                }
                self.ha.due = Some(Instant::now() + AFTER_CLICK);
            }
        }
        self.sync_timers();
    }
}
