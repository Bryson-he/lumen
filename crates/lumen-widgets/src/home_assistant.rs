//! Talking to Home Assistant's web API: checking the connection, reading
//! entities, and calling the service a click stands for. Every call here
//! waits on the network, so none is made on the thread that draws.

use std::collections::HashMap;

use lumen_ipc::home_assistant::{HaConfig, HaEntity, HaValue, click_service, is_clickable, is_entity_id, is_run_only};
use lumen_win::http::{Request, Response, request};

const TIMEOUT_MS: i32 = 8000;
/// A house with thousands of entities sends a long list.
const MAX_ANSWER: usize = 16 * 1024 * 1024;

/// Why a call failed, told apart because Lumen behaves differently for each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaError {
    /// Home Assistant answered and refused the token. Asking again will not help, and each try is logged there.
    Refused,
    /// It could not be reached, or answered with something unexpected.
    Unreachable(String),
}

impl HaError {
    /// The message for the person, in plain words.
    pub fn message(&self) -> String {
        match self {
            Self::Refused => "Home Assistant did not accept the token. Make a new long-lived access token in Home Assistant (your profile, then Security) and paste the whole of it.".to_owned(),
            Self::Unreachable(why) => format!("Could not reach Home Assistant: {why}"),
        }
    }
}

fn call(config: &HaConfig, method: &str, path: &str, body: Option<&[u8]>) -> Result<Response, HaError> {
    let headers = [("Authorization".to_owned(), format!("Bearer {}", config.token)), ("Content-Type".to_owned(), "application/json".to_owned())];
    let answer = request(&Request { url: &format!("{}{path}", config.address), method, headers: &headers, body, timeout_ms: TIMEOUT_MS, max_bytes: MAX_ANSWER }).map_err(HaError::Unreachable)?;
    match answer.status {
        401 | 403 => Err(HaError::Refused),
        _ => Ok(answer),
    }
}

fn json(answer: &Response) -> Result<serde_json::Value, HaError> {
    serde_json::from_slice(&answer.body).map_err(|_| HaError::Unreachable("the answer was not from Home Assistant (is the address right?)".to_owned()))
}

/// Tries the address and token. Gives back what Home Assistant calls itself.
pub fn check(config: &HaConfig) -> Result<String, HaError> {
    let answer = call(config, "GET", "/api/config", None)?;
    if answer.status != 200 {
        return Err(HaError::Unreachable(format!("it answered with an error ({}); is the address right?", answer.status)));
    }
    Ok(json(&answer)?.get("location_name").and_then(serde_json::Value::as_str).unwrap_or("Home Assistant").chars().take(60).collect())
}

/// Reads the entities asked for. One that Home Assistant does not have is left out.
pub fn read(config: &HaConfig, entities: &[String]) -> Result<HashMap<String, HaValue>, HaError> {
    let mut values = HashMap::new();
    for entity in entities.iter().filter(|entity| is_entity_id(entity)) {
        let answer = call(config, "GET", &format!("/api/states/{entity}"), None)?;
        if answer.status == 200 {
            values.insert(entity.clone(), HaValue::from_json(&json(&answer)?));
        }
    }
    Ok(values)
}

/// Everything Home Assistant knows about, for the list to pick from, sorted by name.
pub fn list(config: &HaConfig) -> Result<Vec<HaEntity>, HaError> {
    let answer = call(config, "GET", "/api/states", None)?;
    if answer.status != 200 {
        return Err(HaError::Unreachable(format!("it answered with an error ({})", answer.status)));
    }
    let all = json(&answer)?;
    let mut entities: Vec<HaEntity> = all
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            let id = entity.get("entity_id")?.as_str()?.to_owned();
            let value = HaValue::from_json(entity);
            // A scene's "state" is only when it last ran, which says nothing useful.
            let state = if is_run_only(&id) { "Run".to_owned() } else { value.shown() };
            is_entity_id(&id).then(|| HaEntity { clickable: is_clickable(&id), name: value.name.clone(), state, id })
        })
        .collect();
    entities.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    Ok(entities)
}

/// Does what a click on an entity stands for: switches it, or runs it.
pub fn click(config: &HaConfig, entity: &str, state: &str) -> Result<(), HaError> {
    if !is_entity_id(entity) {
        return Err(HaError::Unreachable(format!("\"{entity}\" is not the name of something in Home Assistant")));
    }
    let (domain, service) = click_service(entity, state);
    let body = serde_json::json!({ "entity_id": entity }).to_string();
    let answer = call(config, "POST", &format!("/api/services/{domain}/{service}"), Some(body.as_bytes()))?;
    if answer.status == 200 { Ok(()) } else { Err(HaError::Unreachable(format!("Home Assistant would not do that ({})", answer.status))) }
}
