//! The weather data source: asking Open-Meteo (free, no account or key) for
//! a forecast and for places by name, reading its answers, and turning a
//! forecast into the text that widget parts and the bar show.
//!
//! The only things ever sent are the latitude and longitude of the place the
//! user chose, the units, and (for a search) the words they typed.

use std::time::{SystemTime, UNIX_EPOCH};

use lumen_ipc::weather::{Place, Units, WeatherConfig, WeatherData, WeatherDay, WeatherState, describe, symbol};
use lumen_win::http;
use serde_json::Value;

const FORECAST_HOST: &str = "api.open-meteo.com";
const SEARCH_HOST: &str = "geocoding-api.open-meteo.com";
const TIMEOUT_MS: i32 = 8000;
/// How often a forecast is fetched while something on screen shows weather.
pub const REFRESH_MS: u32 = 20 * 60 * 1000;
/// A forecast older than this is fetched again as soon as it is needed.
pub const STALE_AFTER_SECS: u64 = 20 * 60;
/// How many days of forecast parts can show, today included.
pub const DAYS: usize = 4;

/// The path and query of a forecast request (kept apart so it can be tested).
fn forecast_path(place: &Place, units: Units) -> String {
    let (temperature, wind) = match units {
        Units::Celsius => ("celsius", "kmh"),
        Units::Fahrenheit => ("fahrenheit", "mph"),
    };
    format!(
        "/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min&forecast_days={DAYS}&timezone=auto&temperature_unit={temperature}&wind_speed_unit={wind}",
        place.latitude, place.longitude
    )
}

/// Fetches the forecast for the configured place. Blocks; run it on a thread of its own.
pub fn fetch(config: &WeatherConfig, now_text: &str) -> Result<WeatherData, String> {
    let place = config.place.as_ref().ok_or("no place is chosen")?;
    // LUMEN_WEATHER_HOST points the request elsewhere, for testing what happens with no connection.
    let host = std::env::var("LUMEN_WEATHER_HOST").unwrap_or_else(|_| FORECAST_HOST.to_owned());
    let body = http::get(&host, &forecast_path(place, config.units), TIMEOUT_MS)?;
    let fetched_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    parse_forecast(&body, config.units, fetched_at, now_text)
}

/// Finds places by name. Blocks for up to a few seconds.
pub fn search(query: &str) -> Result<Vec<Place>, String> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let body = http::get(SEARCH_HOST, &format!("/v1/search?name={}&count=8&language=en&format=json", http::encode(query)), TIMEOUT_MS)?;
    parse_places(&body)
}

fn parse_forecast(body: &str, units: Units, fetched_at: u64, fetched_time: &str) -> Result<WeatherData, String> {
    let unreadable = || "the weather service sent something Lumen could not read".to_owned();
    let doc: Value = serde_json::from_str(body).map_err(|_| unreadable())?;
    let current = doc.get("current").ok_or_else(unreadable)?;
    let number = |value: &Value, key: &str| value.get(key).and_then(Value::as_f64).map(|n| n as f32);
    let daily = doc.get("daily").ok_or_else(unreadable)?;
    let list = |key: &str| daily.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let (dates, codes, highs, lows) = (list("time"), list("weather_code"), list("temperature_2m_max"), list("temperature_2m_min"));
    let days = (0..dates.len().min(DAYS))
        .filter_map(|index| {
            Some(WeatherDay {
                name: if index == 0 { "Today".to_owned() } else { weekday(dates[index].as_str()?)?.to_owned() },
                code: codes.get(index)?.as_u64()? as u8,
                high: highs.get(index)?.as_f64()? as f32,
                low: lows.get(index)?.as_f64()? as f32,
            })
        })
        .collect();
    Ok(WeatherData {
        temperature: number(current, "temperature_2m").ok_or_else(unreadable)?,
        feels_like: number(current, "apparent_temperature").ok_or_else(unreadable)?,
        code: current.get("weather_code").and_then(Value::as_u64).ok_or_else(unreadable)? as u8,
        is_day: current.get("is_day").and_then(Value::as_u64).unwrap_or(1) == 1,
        wind: number(current, "wind_speed_10m").unwrap_or(0.0),
        humidity: number(current, "relative_humidity_2m").unwrap_or(0.0).clamp(0.0, 100.0) as u8,
        days,
        units,
        fetched_at,
        fetched_time: fetched_time.to_owned(),
    })
}

fn parse_places(body: &str) -> Result<Vec<Place>, String> {
    let doc: Value = serde_json::from_str(body).map_err(|_| "the place search sent something Lumen could not read".to_owned())?;
    let results = doc.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|place| {
            let text = |key: &str| place.get(key).and_then(Value::as_str).filter(|text| !text.is_empty());
            // "London, England, United Kingdom", leaving out a region that repeats the name.
            let name = text("name")?;
            let mut pieces = vec![name];
            pieces.extend(text("admin1").filter(|region| *region != name));
            pieces.extend(text("country"));
            Some(Place { name: pieces.join(", "), latitude: place.get("latitude")?.as_f64()?, longitude: place.get("longitude")?.as_f64()? })
        })
        .collect())
}

/// The day of the week of a date written "2026-11-14" (Sakamoto's method).
fn weekday(date: &str) -> Option<&'static str> {
    let mut pieces = date.split('-').map(|piece| piece.parse::<i32>().ok());
    let (mut year, month, day) = (pieces.next()??, pieces.next()??, pieces.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if month < 3 {
        year -= 1;
    }
    let index = (year + year / 4 - year / 100 + year / 400 + OFFSETS[(month - 1) as usize] + day).rem_euclid(7);
    Some(["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][index as usize])
}

/// The live values a part or bar module can show, as `(name, label)`.
pub const SOURCES: [(&str, &str); 22] = [
    ("weather.temp", "Temperature"),
    ("weather.condition", "Conditions"),
    ("weather.icon", "Weather picture"),
    ("weather.feels", "Feels like"),
    ("weather.high", "Today's high"),
    ("weather.low", "Today's low"),
    ("weather.wind", "Wind"),
    ("weather.humidity", "Humidity"),
    ("weather.place", "Place"),
    ("weather.updated", "Last updated"),
    ("weather.day1.name", "Tomorrow: day"),
    ("weather.day1.icon", "Tomorrow: picture"),
    ("weather.day1.high", "Tomorrow: high"),
    ("weather.day1.low", "Tomorrow: low"),
    ("weather.day2.name", "In 2 days: day"),
    ("weather.day2.icon", "In 2 days: picture"),
    ("weather.day2.high", "In 2 days: high"),
    ("weather.day2.low", "In 2 days: low"),
    ("weather.day3.name", "In 3 days: day"),
    ("weather.day3.icon", "In 3 days: picture"),
    ("weather.day3.high", "In 3 days: high"),
    ("weather.day3.low", "In 3 days: low"),
];

/// A weather value as text. Before weather is set up, or before the first
/// forecast arrives, the main values say so and the rest are a dash.
pub fn value_of(source: &str, state: &WeatherState) -> Option<String> {
    let name = source.strip_prefix("weather.")?;
    let degrees = |value: f32| format!("{}°", value.round() as i32);
    let Some(data) = &state.data else {
        let waiting = if !state.config.enabled {
            "Weather is off"
        } else if state.config.place.is_none() {
            "Choose a place in Lumen"
        } else if state.error.is_some() {
            "No connection"
        } else {
            "Loading…"
        };
        return Some(match name {
            "condition" => waiting.to_owned(),
            "place" => state.config.place.as_ref().map_or(String::new(), |place| short_place(&place.name)),
            _ if SOURCES.iter().any(|(known, _)| *known == source) => "–".to_owned(),
            _ => return None,
        });
    };
    let day = |index: usize, field: &str| {
        let day = data.days.get(index)?;
        Some(match field {
            "name" => day.name.clone(),
            "icon" => symbol(day.code, true).to_owned(),
            "high" => degrees(day.high),
            "low" => degrees(day.low),
            _ => return None,
        })
    };
    Some(match name {
        "temp" => degrees(data.temperature),
        "condition" => describe(data.code).to_owned(),
        "icon" => symbol(data.code, data.is_day).to_owned(),
        "feels" => degrees(data.feels_like),
        "high" => day(0, "high").unwrap_or_else(|| "–".to_owned()),
        "low" => day(0, "low").unwrap_or_else(|| "–".to_owned()),
        "wind" => format!("{} {}", data.wind.round() as i32, if data.units == Units::Celsius { "km/h" } else { "mph" }),
        "humidity" => format!("{}%", data.humidity),
        "place" => state.config.place.as_ref().map_or(String::new(), |place| short_place(&place.name)),
        "updated" => if state.error.is_some() { format!("Offline · last {}", data.fetched_time) } else { format!("Updated {}", data.fetched_time) },
        other => {
            let (which, field) = other.split_once('.')?;
            let index = which.strip_prefix("day")?.parse::<usize>().ok()?;
            day(index, field).unwrap_or_else(|| "–".to_owned())
        }
    })
}

/// "London, England, United Kingdom" → "London".
fn short_place(name: &str) -> String {
    name.split(',').next().unwrap_or(name).trim().to_owned()
}

/// One line for the bar: "⛅ 18° Partly cloudy".
pub fn bar_text(state: &WeatherState) -> String {
    match &state.data {
        Some(_) => format!("{} {}  {}", value_of("weather.icon", state).unwrap_or_default(), value_of("weather.temp", state).unwrap_or_default(), value_of("weather.condition", state).unwrap_or_default()),
        None => value_of("weather.condition", state).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORECAST: &str = r#"{"latitude":51.5,"longitude":-0.12,"timezone":"Europe/London",
        "current":{"time":"2026-11-14T10:45","temperature_2m":11.6,"apparent_temperature":9.2,"relative_humidity_2m":81,"weather_code":2,"wind_speed_10m":14.4,"is_day":1},
        "daily":{"time":["2026-11-14","2026-11-15","2026-11-16","2026-11-17"],"weather_code":[2,61,3,0],"temperature_2m_max":[12.8,10.4,9.1,8.6],"temperature_2m_min":[7.2,6.5,4.9,2.1]}}"#;

    fn london() -> WeatherConfig {
        WeatherConfig { enabled: true, place: Some(Place { name: "London, England, United Kingdom".into(), latitude: 51.50853, longitude: -0.12574 }), units: Units::Celsius }
    }

    #[test]
    fn a_request_carries_only_the_place_and_the_units() {
        let path = forecast_path(london().place.as_ref().unwrap(), Units::Fahrenheit);
        assert!(path.starts_with("/v1/forecast?latitude=51.5085&longitude=-0.1257&"), "{path}");
        assert!(path.contains("temperature_unit=fahrenheit") && path.contains("wind_speed_unit=mph") && path.contains("forecast_days=4"));
        assert!(!path.contains("London"), "the place's name is not sent");
    }

    #[test]
    fn a_forecast_is_read_into_values_parts_can_show() {
        let data = parse_forecast(FORECAST, Units::Celsius, 1_800_000_000, "10:45").unwrap();
        assert_eq!((data.temperature, data.code, data.is_day, data.humidity, data.days.len()), (11.6, 2, true, 81, 4));
        let state = WeatherState { config: london(), data: Some(data), error: None };
        let shows = |source: &str| value_of(source, &state).unwrap();
        assert_eq!((shows("weather.temp"), shows("weather.feels"), shows("weather.condition")), ("12°".to_owned(), "9°".to_owned(), "Partly cloudy".to_owned()));
        assert_eq!((shows("weather.high"), shows("weather.low"), shows("weather.wind"), shows("weather.humidity")), ("13°".to_owned(), "7°".to_owned(), "14 km/h".to_owned(), "81%".to_owned()));
        assert_eq!((shows("weather.place"), shows("weather.updated")), ("London".to_owned(), "Updated 10:45".to_owned()));
        assert_eq!((shows("weather.day1.name"), shows("weather.day1.high"), shows("weather.day3.name"), shows("weather.day3.low")), ("Sun".to_owned(), "10°".to_owned(), "Tue".to_owned(), "2°".to_owned()));
        assert_eq!(shows("weather.day9.high"), "–", "a day past the forecast is a dash");
        assert_eq!(value_of("weather.nonsense", &state), None);
        assert_eq!(value_of("clock.time", &state), None);
        assert!(bar_text(&state).ends_with("12°  Partly cloudy"));
        for (source, _) in SOURCES {
            assert!(value_of(source, &state).is_some(), "{source}");
        }
    }

    #[test]
    fn before_a_forecast_arrives_the_widget_says_why() {
        let says = |state: &WeatherState| value_of("weather.condition", state).unwrap();
        assert_eq!(says(&WeatherState::default()), "Weather is off");
        assert_eq!(says(&WeatherState { config: WeatherConfig { enabled: true, ..Default::default() }, ..Default::default() }), "Choose a place in Lumen");
        let waiting = WeatherState { config: london(), data: None, error: None };
        assert_eq!((says(&waiting), value_of("weather.temp", &waiting).unwrap(), value_of("weather.place", &waiting).unwrap()), ("Loading…".to_owned(), "–".to_owned(), "London".to_owned()));
        assert_eq!(says(&WeatherState { error: Some("no internet".into()), ..waiting }), "No connection");
    }

    #[test]
    fn an_old_forecast_stays_up_while_offline_and_says_so() {
        let data = parse_forecast(FORECAST, Units::Celsius, 1_800_000_000, "10:45").unwrap();
        let state = WeatherState { config: london(), data: Some(data), error: Some("could not reach the service".into()) };
        assert_eq!(value_of("weather.temp", &state).unwrap(), "12°");
        assert_eq!(value_of("weather.updated", &state).unwrap(), "Offline · last 10:45");
    }

    #[test]
    fn places_are_read_with_their_region_and_country() {
        let body = r#"{"results":[{"name":"London","latitude":51.50853,"longitude":-0.12574,"admin1":"England","country":"United Kingdom"},
            {"name":"London","latitude":42.98339,"longitude":-81.23304,"admin1":"Ontario","country":"Canada"},
            {"name":"Singapore","latitude":1.28967,"longitude":103.85007,"admin1":"Singapore","country":"Singapore"},{"name":"Broken"}]}"#;
        let places = parse_places(body).unwrap();
        assert_eq!(places.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["London, England, United Kingdom", "London, Ontario, Canada", "Singapore, Singapore"]);
        assert_eq!(parse_places("{}").unwrap(), [], "no results is an empty list");
        assert!(parse_places("not json").is_err() && parse_forecast("{}", Units::Celsius, 0, "").is_err());
        assert!(search("a").unwrap().is_empty(), "one letter is not searched for");
    }

    #[test]
    fn dates_become_weekdays() {
        assert_eq!((weekday("2026-11-14"), weekday("2026-11-15"), weekday("2024-02-29"), weekday("2000-01-01")), (Some("Sat"), Some("Sun"), Some("Thu"), Some("Sat")));
        assert_eq!((weekday("nonsense"), weekday("2026-13-01")), (None, None));
    }
}
