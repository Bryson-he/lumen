//! Weather in the host: the settings, when a forecast is fetched, and what
//! is kept between fetches.
//!
//! Lumen goes online for weather only while all of these hold: weather is
//! switched on, a place is chosen, something on screen shows weather, and no
//! fullscreen app or game is in front. The last forecast is kept on disk, so
//! it is there at start-up and while offline.

use std::time::{SystemTime, UNIX_EPOCH};

use lumen_ipc::weather::{WeatherConfig, WeatherData, WeatherState};
use serde::{Deserialize, Serialize};

use super::*;
use crate::weather;

pub(super) const TIMER_WEATHER: usize = 8;
/// After a failed fetch, try again soon, then less and less often: a
/// connection that drops the odd request recovers in seconds, and one that is
/// really down is not pestered.
const RETRY_FIRST_SECS: u64 = 10;
const RETRY_LONGEST_SECS: u64 = 300;

/// How long to wait before trying again after this many failures in a row.
fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs((RETRY_FIRST_SECS << failures.saturating_sub(1).min(8)).min(RETRY_LONGEST_SECS))
}

#[derive(Default)]
pub(super) struct Weather {
    pub(super) state: WeatherState,
    fetching: bool,
    /// When the latest fetch failed, if it did, and how many have failed in a row.
    failed_at: Option<Instant>,
    failures: u32,
}

/// `weather-cache.json`: the last forecast, and which request it answers.
#[derive(Serialize, Deserialize)]
struct Cache {
    config: WeatherConfig,
    data: WeatherData,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Host {
    pub(super) fn load_weather(&mut self) {
        let read = |path: &std::path::Path| std::fs::read_to_string(path).ok().map(|text| text.trim_start_matches('\u{feff}').to_owned());
        let config = read(&self.options.weather_path).and_then(|text| serde_json::from_str::<WeatherConfig>(&text).ok()).map(WeatherConfig::sanitised).unwrap_or_default();
        // A kept forecast only counts if it is for the same place and units.
        let cached = read(&cache_path(&self.options.weather_path)).and_then(|text| serde_json::from_str::<Cache>(&text).ok()).filter(|cache| same_request(&cache.config, &config));
        self.weather.state = WeatherState { data: cached.map(|cache| cache.data), config, error: None };
    }

    /// Applies new weather settings: saves them, drops a forecast for another
    /// place, and fetches if something is showing weather.
    pub(super) fn set_weather(&mut self, config: WeatherConfig) {
        let config = config.sanitised();
        if config == self.weather.state.config {
            return;
        }
        if !same_request(&config, &self.weather.state.config) {
            self.weather.state.data = None;
        }
        self.weather.state.error = None;
        self.weather.failed_at = None;
        self.weather.failures = 0;
        self.weather.state.config = config;
        match serde_json::to_string_pretty(&self.weather.state.config) {
            Ok(text) => {
                if let Err(error) = std::fs::write(&self.options.weather_path, text) {
                    warn!("could not save {}: {error}", self.options.weather_path.display());
                }
            }
            Err(error) => warn!("{error}"),
        }
        self.weather_changed();
        self.sync_timers();
    }

    /// Fetch now, if weather is set up (the "Refresh" button).
    pub(super) fn refresh_weather(&mut self) {
        self.weather.failed_at = None;
        self.weather.failures = 0;
        if self.weather.state.config.active() && !self.paused {
            self.start_weather_fetch();
        }
    }

    /// Whether anything on screen shows weather right now.
    fn weather_on_screen(&self) -> bool {
        self.any_visible(Kind::Weather) || (self.any_bar_showing() && bar::needs_weather(&self.bar))
    }

    /// How long until the next fetch is due, or `None` if none is wanted.
    pub(super) fn weather_due_in(&self) -> Option<u32> {
        let weather = &self.weather;
        if !weather.state.config.active() || self.paused || weather.fetching || !self.weather_on_screen() {
            return None;
        }
        let age = weather.state.data.as_ref().map(|data| now_secs().saturating_sub(data.fetched_at));
        let until_stale = age.map_or(0, |age| weather::STALE_AFTER_SECS.saturating_sub(age));
        let until_retry = weather.failed_at.map_or(0, |at| retry_delay(weather.failures).saturating_sub(at.elapsed()).as_secs());
        // Never quite zero: a moment for whatever asked to finish first.
        Some((until_stale.max(until_retry).min(u64::from(weather::REFRESH_MS / 1000)) * 1000).max(400) as u32)
    }

    /// Handles the weather timer; `false` if the timer is not this one.
    pub(super) fn on_weather_timer(&mut self, timer: usize) -> bool {
        if timer != TIMER_WEATHER {
            return false;
        }
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_WEATHER);
        }
        if self.weather_due_in().is_some() {
            self.start_weather_fetch();
        }
        true
    }

    fn start_weather_fetch(&mut self) {
        if self.weather.fetching {
            return;
        }
        self.weather.fetching = true;
        let config = self.weather.state.config.clone();
        let handle = HostHandle { window: self.window.0 as isize, inbox: self.inbox.clone() };
        let spawned = std::thread::Builder::new().name("weather".into()).spawn(move || {
            let result = weather::fetch(&config, "");
            handle.set_weather(config, result);
        });
        if spawned.is_err() {
            self.weather.fetching = false;
        }
    }

    /// A fetch finished (on its own thread; this runs on the host's).
    pub(super) fn on_weather_result(&mut self, config: WeatherConfig, result: Result<WeatherData, String>) {
        self.weather.fetching = false;
        // The settings changed while it was away: this answer is for something else.
        if same_request(&config, &self.weather.state.config) {
            match result {
                Ok(mut data) => {
                    data.fetched_time = if self.clock.suffix.is_empty() { self.clock.time.clone() } else { format!("{} {}", self.clock.time, self.clock.suffix) };
                    if let Ok(text) = serde_json::to_string(&Cache { config: config.clone(), data: data.clone() }) {
                        let _ = std::fs::write(cache_path(&self.options.weather_path), text);
                    }
                    self.weather.state.data = Some(data);
                    self.weather.state.error = None;
                    self.weather.failed_at = None;
                    self.weather.failures = 0;
                }
                Err(error) => {
                    info!("weather: {error}");
                    self.weather.state.error = Some(error);
                    self.weather.failed_at = Some(Instant::now());
                    self.weather.failures += 1;
                }
            }
            self.weather_changed();
        }
        self.sync_timers();
    }

    fn weather_changed(&mut self) {
        self.draw(Some(Kind::Weather));
        (self.on_state)(Report::Weather(self.weather.state.clone()));
    }
}

/// Whether two settings ask the service for the same thing.
fn same_request(a: &WeatherConfig, b: &WeatherConfig) -> bool {
    a.place == b.place && a.units == b.units
}

/// `weather-cache.json`, beside `weather.json`.
fn cache_path(config: &std::path::Path) -> PathBuf {
    config.with_file_name("weather-cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_start_quick_and_back_off() {
        let secs: Vec<u64> = (1..=8).map(|failures| retry_delay(failures).as_secs()).collect();
        assert_eq!(secs, [10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(retry_delay(0).as_secs(), 10);
        assert_eq!(retry_delay(u32::MAX).as_secs(), 300);
    }
}
