//! `lumen-core`: the always-on Lumen service.
//!
//! Two threads:
//! - **main**: Win32 message loop for the tray icon and the fullscreen watcher.
//! - **service**: a single-threaded tokio runtime with the state, the IPC
//!   server and the preview timer. Everything that touches the desktop runs here.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;
mod ipc;
mod service;
mod store;
mod tray;

use std::{cell::RefCell, fs, process::ExitCode, rc::Rc, sync::Mutex, thread};

use tokio::{
    sync::{mpsc, oneshot},
    task::LocalSet,
};
use tracing::{error, info};

use crate::{desktop::WindowsDesktop, service::Service, store::Store};

fn main() -> ExitCode {
    let store = match Store::open(Store::default_root()) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("lumen-core: could not open the data folder {}: {error}", Store::default_root().display());
            return ExitCode::FAILURE;
        }
    };
    start_logging(&store);
    info!("lumen-core {} starting; data in {}", env!("CARGO_PKG_VERSION"), store.root().display());

    // Fullscreen changes flow main -> service; shutdown flows main -> service;
    // "ready or failed" flows service -> main once.
    let (game_tx, game_rx) = mpsc::unbounded_channel::<Option<String>>();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let service_thread = thread::Builder::new()
        .name("service".into())
        .spawn(move || run_service(store, game_rx, shutdown_rx, ready_tx))
        .expect("the service thread starts");

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            error!("{message}");
            eprintln!("lumen-core: {message}");
            return ExitCode::FAILURE;
        }
        Err(_) => return ExitCode::FAILURE,
    }

    let tray = match tray::Tray::new() {
        Ok(tray) => Some(tray),
        Err(message) => {
            // Still useful without an icon; the settings app can drive it.
            error!("no tray icon: {message}");
            None
        }
    };

    // Blocks here, pumping this thread's messages (tray included), until Quit.
    let watching = lumen_win::fullscreen::watch(None, move |app| {
        let _ = game_tx.send(app.map(|app| app.title.clone()));
    });
    if let Err(error) = watching {
        error!("fullscreen watcher: {error}");
    }

    drop(tray);
    let _ = shutdown_tx.send(());
    let _ = service_thread.join();
    info!("lumen-core stopped");
    ExitCode::SUCCESS
}

fn run_service(
    store: Store,
    mut game_rx: mpsc::UnboundedReceiver<Option<String>>,
    shutdown_rx: oneshot::Receiver<()>,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_io().enable_time().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_tx.send(Err(format!("could not start the runtime: {error}")));
            return;
        }
    };

    LocalSet::new().block_on(&runtime, async move {
        let pipe_name = lumen_ipc::pipe_name();
        let listener = match ipc::listen(&pipe_name) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = ready_tx.send(Err(format!("Lumen is already running, or the pipe {pipe_name} is unavailable ({error})")));
                return;
            }
        };
        let desktop = match WindowsDesktop::new() {
            Ok(desktop) => desktop,
            Err(message) => {
                let _ = ready_tx.send(Err(format!("could not reach the Windows desktop: {message}")));
                return;
            }
        };
        let service = Rc::new(RefCell::new(Service::new(store, desktop)));
        info!("listening on {pipe_name}");
        let _ = ready_tx.send(Ok(()));

        tokio::task::spawn_local(ipc::serve(listener, pipe_name, service.clone()));
        tokio::task::spawn_local(service::auto_revert(service.clone()));
        let game_service = service.clone();
        tokio::task::spawn_local(async move {
            while let Some(app) = game_rx.recv().await {
                game_service.borrow_mut().set_game_mode(app);
            }
        });

        let _ = shutdown_rx.await;
        service.borrow_mut().shutdown();
        // Let apps finish hearing about the last colour change before exiting.
        lumen_win::flush_broadcasts();
    });
}

/// Logs to `logs/lumen-core.log` in the data folder (and to the console in
/// debug builds). The file starts fresh once it passes 1 MB.
fn start_logging(store: &Store) {
    let dir = store.root().join("logs");
    let path = dir.join("lumen-core.log");
    let _ = fs::create_dir_all(&dir);
    if fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = fs::rename(&path, dir.join("lumen-core.old.log"));
    }
    let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(&path) else { return };

    let level = if cfg!(debug_assertions) { tracing::Level::DEBUG } else { tracing::Level::INFO };
    let builder = tracing_subscriber::fmt().with_max_level(level).with_ansi(false).with_target(false);
    if cfg!(debug_assertions) {
        builder.with_writer(Mutex::new(Tee(file))).init();
    } else {
        builder.with_writer(Mutex::new(file)).init();
    }
}

/// Writes log lines to the file and to stderr.
struct Tee(fs::File);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
