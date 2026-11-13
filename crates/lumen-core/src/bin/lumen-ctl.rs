//! `lumen-ctl`: talk to a running `lumen-core` from the command line.

use std::{process::ExitCode, time::Duration};

use lumen_ipc::Request;
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::ClientOptions,
    time::Instant,
};

const USAGE: &str = "\
lumen-ctl: send a request to the running lumen-core

Usage:
  lumen-ctl <method> [json-params] [--listen <seconds>]
  lumen-ctl listen <seconds>

Methods:
  getTheme
  previewTheme     '{\"source\":{\"type\":\"color\",\"value\":\"#2F5BEA\"},\"variant\":\"tonal\",\"mode\":\"dark\"}'
  commitPreview
  revertPreview
  listWallpapers
  importWallpaper  '{\"path\":\"C:/Pictures/harbour.jpg\"}'
  applyPreset      '{\"id\":\"midnight-harbour\"}'

--listen keeps the connection open and prints events (themeChanged,
gameModeChanged) with the time each one arrived.";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build().expect("runtime");
    match runtime.block_on(run(&mut args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

async fn run(args: &mut Vec<String>) -> Result<(), String> {
    let mut listen = None;
    if let Some(index) = args.iter().position(|a| a == "--listen") {
        let value = args.get(index + 1).cloned().ok_or("--listen needs a number of seconds")?;
        listen = Some(seconds(&value)?);
        args.drain(index..=index + 1);
    }
    let request = if args[0] == "listen" {
        listen = Some(seconds(args.get(1).ok_or("listen needs a number of seconds")?)?);
        None
    } else {
        let params = match args.get(1) {
            Some(text) => serde_json::from_str(text).map_err(|e| format!("the params are not valid JSON: {e}"))?,
            None => Value::Null,
        };
        Some(Request::new(1, &args[0], params))
    };

    let name = lumen_ipc::pipe_name();
    let pipe = ClientOptions::new().open(&name).map_err(|e| format!("could not connect to lumen-core on {name}: {e}. Is it running?"))?;
    let (reader, mut writer) = tokio::io::split(pipe);
    let mut lines = BufReader::new(reader).lines();
    let started = Instant::now();

    let mut failed = false;
    if let Some(request) = &request {
        let line = serde_json::to_string(request).map_err(|e| e.to_string())? + "\n";
        writer.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        // Events may arrive before the reply; print them and keep waiting.
        loop {
            let line = lines.next_line().await.map_err(|e| e.to_string())?.ok_or("lumen-core closed the connection")?;
            let message: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if message.get("id").is_some_and(|id| !id.is_null()) || message.get("error").is_some() {
                if let Some(error) = message.get("error") {
                    eprintln!("error {}: {}", error["code"], error["message"].as_str().unwrap_or_default());
                    failed = true;
                } else {
                    println!("{}", serde_json::to_string_pretty(&message["result"]).unwrap_or_default());
                }
                break;
            }
            print_event(started, &message);
        }
    }

    if let Some(duration) = listen {
        let until = started + duration;
        while let Ok(Ok(Some(line))) = tokio::time::timeout_at(until, lines.next_line()).await {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                print_event(started, &message);
            }
        }
    }
    if failed { Err("the request failed".into()) } else { Ok(()) }
}

fn print_event(started: Instant, message: &Value) {
    let method = message["method"].as_str().unwrap_or("?");
    let params = &message["params"];
    let summary = match method {
        "themeChanged" => format!(
            "reason={} preview={} theme={}",
            params["reason"].as_str().unwrap_or("?"),
            params["preview"],
            params["theme"]["name"].as_str().map_or("none".to_owned(), |n| format!("\"{n}\"")),
        ),
        _ => params.to_string(),
    };
    println!("[{:>5.1}s] event {method}: {summary}", started.elapsed().as_secs_f64());
}

fn seconds(text: &str) -> Result<Duration, String> {
    match text.parse::<f64>() {
        Ok(s) if s > 0.0 && s <= 3600.0 => Ok(Duration::from_secs_f64(s)),
        _ => Err(format!("expected a number of seconds, got \"{text}\"")),
    }
}
