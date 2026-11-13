//! Weather: the shape of `weather.json`, of a forecast, and of the weather
//! messages between the core and the settings app.

use serde::{Deserialize, Serialize};

/// A place on the map, as chosen from a search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    /// "London, England, United Kingdom".
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Units {
    /// °C and km/h.
    #[default]
    Celsius,
    /// °F and mph.
    Fahrenheit,
}

/// `weather.json`. Lumen only goes online for weather while `enabled` is on
/// and a `place` is chosen.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub place: Option<Place>,
    pub units: Units,
}

impl WeatherConfig {
    /// Whether there is anything to fetch.
    pub fn active(&self) -> bool {
        self.enabled && self.place.is_some()
    }

    pub fn sanitised(mut self) -> Self {
        self.place = self.place.filter(|place| place.latitude.is_finite() && place.longitude.is_finite() && place.latitude.abs() <= 90.0 && place.longitude.abs() <= 180.0);
        if let Some(place) = &mut self.place {
            place.name = place.name.trim().chars().take(120).collect();
        }
        self
    }
}

/// One day of the forecast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherDay {
    /// "Sun", "Mon"…
    pub name: String,
    /// The World Meteorological Organization's weather code.
    pub code: u8,
    pub high: f32,
    pub low: f32,
}

/// A forecast as fetched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherData {
    /// In the units asked for.
    pub temperature: f32,
    pub feels_like: f32,
    pub code: u8,
    pub is_day: bool,
    pub wind: f32,
    /// Percent.
    pub humidity: u8,
    /// Today first.
    pub days: Vec<WeatherDay>,
    pub units: Units,
    /// When it was fetched: seconds since 1970, and "10:42" in local time.
    pub fetched_at: u64,
    pub fetched_time: String,
}

/// What `getWeather` returns and `weatherChanged` carries.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherState {
    pub config: WeatherConfig,
    /// The last forecast fetched for this place, if any (kept while offline).
    pub data: Option<WeatherData>,
    /// Why the latest fetch failed, if it did.
    pub error: Option<String>,
}

/// A weather code in words.
pub fn describe(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Cloudy",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 => "Showers",
        82 => "Heavy showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm, hail",
        _ => "Unknown",
    }
}

/// A weather code as a small picture (an emoji, drawn in colour).
pub fn symbol(code: u8, is_day: bool) -> &'static str {
    match code {
        0 | 1 if is_day => "☀️",
        0 | 1 => "🌙",
        2 if is_day => "⛅",
        2 | 3 => "☁️",
        45 | 48 => "🌫️",
        51..=57 => "🌦️",
        61..=67 | 80..=82 => "🌧️",
        71..=77 | 85 | 86 => "❄️",
        95..=99 => "⛈️",
        _ => "·",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_fetched_until_weather_is_on_and_a_place_is_chosen() {
        let place = Place { name: "London".into(), latitude: 51.5, longitude: -0.12 };
        assert!(!WeatherConfig::default().active());
        assert!(!WeatherConfig { enabled: true, place: None, ..Default::default() }.active());
        assert!(!WeatherConfig { enabled: false, place: Some(place.clone()), ..Default::default() }.active());
        assert!(WeatherConfig { enabled: true, place: Some(place), ..Default::default() }.active());
    }

    #[test]
    fn a_place_off_the_map_is_dropped() {
        let config = WeatherConfig { enabled: true, place: Some(Place { name: " Nowhere ".into(), latitude: 200.0, longitude: 0.0 }), ..Default::default() }.sanitised();
        assert_eq!(config.place, None);
        let config = WeatherConfig { place: Some(Place { name: " Here ".into(), latitude: -33.9, longitude: 151.2 }), ..Default::default() }.sanitised();
        assert_eq!(config.place.unwrap().name, "Here");
    }

    #[test]
    fn codes_have_words_and_pictures() {
        assert_eq!((describe(0), describe(63), describe(95), describe(200)), ("Clear", "Rain", "Thunderstorm", "Unknown"));
        assert_ne!(symbol(0, true), symbol(0, false), "sun by day, moon by night");
        assert_eq!(symbol(65, true), symbol(80, false));
    }
}
