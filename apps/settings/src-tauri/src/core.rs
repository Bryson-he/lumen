//! The settings app's connection to `lumen-core`.
//!
//! One background task owns the named pipe. The web page calls [`core_call`]
//! for requests; events from the core are re-emitted to the page as the Tauri
//! event `core-event`, and connection changes as `core-connection`.
//! If the core is not running, the app starts the `lumen-core.exe` next to it.

use std::{collections::HashMap, time::Duration};

use lumen_ipc::{Request, method};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    sync::{mpsc, oneshot},
};

/// Windows: "all pipe instances are busy".
const ERROR_PIPE_BUSY: i32 = 231;
/// Request id the backend uses for its own `getStatus` on connect.
const HELLO_ID: u64 = 0;

/// An error the page can show: a JSON-RPC error from the core, or code 0 when
/// the core could not be reached at all.
#[derive(Debug, Clone, Serialize)]
pub struct CallError {
    pub code: i32,
    pub message: String,
}

impl CallError {
    fn offline(message: impl Into<String>) -> Self {
        Self { code: 0, message: message.into() }
    }
}

struct Call {
    method: String,
    params: Value,
    reply: oneshot::Sender<Result<Value, CallError>>,
}

/// Managed state: the queue of requests for the connection task.
pub struct Core {
    calls: mpsc::UnboundedSender<Call>,
}

#[derive(Clone, Serialize)]
struct Connection {
    connected: bool,
    message: Option<String>,
}

/// Sends one request to the core and waits for its answer.
#[tauri::command]
pub async fn core_call(core: State<'_, Core>, method: String, params: Option<Value>) -> Result<Value, CallError> {
    let (reply, answer) = oneshot::channel();
    core.calls
        .send(Call { method, params: params.unwrap_or(Value::Null), reply })
        .map_err(|_| CallError::offline("the connection to Lumen has stopped"))?;
    answer.await.map_err(|_| CallError::offline("Lumen closed the connection before answering"))?
}

/// Starts the connection task. Call once, at app setup.
pub fn start(app: &AppHandle) {
    let (calls, queue) = mpsc::unbounded_channel();
    app.manage(Core { calls });
    tauri::async_runtime::spawn(run(app.clone(), queue));
}

async fn run(app: AppHandle, mut queue: mpsc::UnboundedReceiver<Call>) {
    loop {
        match connect().await {
            Ok(pipe) => {
                let _ = app.emit("core-connection", Connection { connected: true, message: None });
                session(&app, pipe, &mut queue).await;
                let _ = app.emit("core-connection", Connection { connected: false, message: Some("Lumen's core stopped. Reconnecting…".into()) });
            }
            Err(message) => {
                let _ = app.emit("core-connection", Connection { connected: false, message: Some(message.clone()) });
                // Answer whatever is waiting rather than leaving the page hanging, then retry.
                let retry_at = tokio::time::Instant::now() + Duration::from_secs(2);
                while let Ok(Some(call)) = tokio::time::timeout_at(retry_at, queue.recv()).await {
                    let _ = call.reply.send(Err(CallError::offline(message.clone())));
                }
            }
        }
        if queue.is_closed() {
            return;
        }
    }
}

/// Opens the pipe, starting the core first if nobody is listening.
async fn connect() -> Result<NamedPipeClient, String> {
    let name = lumen_ipc::pipe_name();
    let mut started_core = false;
    let mut last_error = String::new();
    for _ in 0..40 {
        match ClientOptions::new().open(&name) {
            Ok(pipe) => return Ok(pipe),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {}
            Err(error) => {
                last_error = error.to_string();
                if !started_core {
                    started_core = true;
                    start_core()?;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(format!("Could not reach Lumen's core ({last_error})"))
}

fn start_core() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("lumen-core.exe");
    if !exe.is_file() {
        return Err(format!("Lumen's core is not running and {} was not found", exe.display()));
    }
    std::process::Command::new(&exe).spawn().map(drop).map_err(|e| format!("could not start {}: {e}", exe.display()))
}

/// Runs one connection until the pipe closes.
async fn session(app: &AppHandle, pipe: NamedPipeClient, queue: &mut mpsc::UnboundedReceiver<Call>) {
    let (reader, mut writer) = tokio::io::split(pipe);
    let mut lines = BufReader::new(reader).lines();
    let mut waiting: HashMap<u64, oneshot::Sender<Result<Value, CallError>>> = HashMap::new();
    let mut next_id = HELLO_ID + 1;

    // Ask where the data folder is, so the page may load wallpaper previews from it.
    if send(&mut writer, &Request::new(HELLO_ID, method::GET_STATUS, Value::Null)).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            call = queue.recv() => {
                let Some(call) = call else { break };
                let id = next_id;
                next_id += 1;
                if send(&mut writer, &Request::new(id, &call.method, call.params)).await.is_err() {
                    let _ = call.reply.send(Err(CallError::offline("Lumen's core stopped")));
                    break;
                }
                waiting.insert(id, call.reply);
            }
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                match message.get("id").and_then(Value::as_u64) {
                    Some(HELLO_ID) => allow_data_folder(app, &message["result"]),
                    Some(id) => {
                        if let Some(reply) = waiting.remove(&id) {
                            let _ = reply.send(answer(&message));
                        }
                    }
                    // No id: an event from the core.
                    None if message.get("method").is_some() => {
                        let _ = app.emit("core-event", &message);
                    }
                    None => {}
                }
            }
        }
    }
    for (_, reply) in waiting {
        let _ = reply.send(Err(CallError::offline("Lumen's core stopped")));
    }
}

async fn send(writer: &mut (impl AsyncWriteExt + Unpin), request: &Request) -> std::io::Result<()> {
    let mut line = serde_json::to_string(request)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await
}

fn answer(message: &Value) -> Result<Value, CallError> {
    match message.get("error") {
        Some(error) if !error.is_null() => Err(CallError {
            code: error["code"].as_i64().unwrap_or(0) as i32,
            message: error["message"].as_str().unwrap_or("Lumen's core reported an error").to_owned(),
        }),
        _ => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
    }
}

/// Lets the page read images from Lumen's data folder (thumbnails), and nothing else on disk.
fn allow_data_folder(app: &AppHandle, status: &Value) {
    if let Some(dir) = status["dataDir"].as_str() {
        let _ = app.asset_protocol_scope().allow_directory(dir, true);
    }
}
