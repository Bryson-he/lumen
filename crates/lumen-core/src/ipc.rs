//! The named-pipe server: JSON-RPC 2.0, one JSON object per line.

use std::{cell::RefCell, io, rc::Rc, time::Duration};

use lumen_ipc::{Request, Response, RpcError, code};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::broadcast::error::RecvError,
    task::spawn_local,
};
use tracing::{debug, warn};

use crate::{desktop::Desktop, service::Service};

/// Creates the pipe. Fails if another `lumen-core` already owns it.
pub fn listen(name: &str) -> io::Result<NamedPipeServer> {
    ServerOptions::new().first_pipe_instance(true).create(name)
}

/// Accepts clients forever, one task per connection.
pub async fn serve<D: Desktop + 'static>(mut listener: NamedPipeServer, name: String, service: Rc<RefCell<Service<D>>>) {
    loop {
        if let Err(error) = listener.connect().await {
            warn!("pipe connect failed: {error}");
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        }
        // Make the next instance before handing this one off, so the pipe never disappears.
        match ServerOptions::new().create(&name) {
            Ok(next) => {
                let client = std::mem::replace(&mut listener, next);
                spawn_local(connection(client, service.clone()));
            }
            Err(error) => {
                warn!("could not create the next pipe instance: {error}");
                let _ = listener.disconnect();
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn connection<D: Desktop>(pipe: NamedPipeServer, service: Rc<RefCell<Service<D>>>) {
    debug!("client connected");
    let mut events = service.borrow().subscribe();
    let (reader, mut writer) = tokio::io::split(pipe);
    let mut lines = BufReader::new(reader).lines();

    loop {
        let outgoing = tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) if line.trim().is_empty() => continue,
                Ok(Some(line)) => match answer(&service, &line) {
                    Some(reply) => reply,
                    None => continue,
                },
                // Client closed the pipe, or it broke.
                Ok(None) | Err(_) => break,
            },
            event = events.recv() => match event {
                Ok(line) => line.to_string(),
                // A very slow client missed some events; carry on with the newest.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
        };
        if writer.write_all(outgoing.as_bytes()).await.is_err() || writer.write_all(b"\n").await.is_err() {
            break;
        }
    }
    debug!("client disconnected");
}

/// Runs one request line and returns the reply line (none for notifications).
fn answer<D: Desktop>(service: &Rc<RefCell<Service<D>>>, line: &str) -> Option<String> {
    let response = match serde_json::from_str::<Request>(line) {
        Err(error) => Response::err(Value::Null, RpcError::new(code::PARSE_ERROR, format!("not a JSON-RPC request: {error}"))),
        Ok(request) if request.jsonrpc != "2.0" => {
            Response::err(request.id.unwrap_or(Value::Null), RpcError::new(code::INVALID_REQUEST, "\"jsonrpc\" must be \"2.0\""))
        }
        Ok(request) => {
            debug!("-> {}", request.method);
            let result = service.borrow_mut().handle(&request.method, request.params);
            let id = request.id?;
            match result {
                Ok(value) => Response::ok(id, value),
                Err(error) => Response::err(id, error),
            }
        }
    };
    serde_json::to_string(&response).ok()
}
