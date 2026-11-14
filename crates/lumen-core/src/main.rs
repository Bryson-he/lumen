//! `lumen-core`: the always-on Lumen service.
//!
//! Two threads:
//! - **main**: Win32 message loop for the tray icon and the fullscreen watcher.
//! - **service**: a single-threaded tokio runtime with the state, the IPC
//!   server and the preview timer. Everything that touches the desktop runs here.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;
mod exports;
mod gallery;
mod ipc;
mod preset_store;
mod service;
mod stats;
mod store;
mod tray;
mod widget_packages;

use std::{
    cell::RefCell,
    fs,
    process::ExitCode,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

use lumen_widgets::host::{self, HostHandle};
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
    // Before any window exists: draw crisply on every monitor, whatever its scaling.
    lumen_render::enable_per_monitor_dpi();
    let layout_path = store.root().join("widgets").join("layout.json");
    let bar_path = store.root().join("bar.json");
    let flyouts_path = store.root().join("flyouts.json");
    let launcher_path = store.root().join("launcher.json");
    let styles_path = store.root().join("styles.json");
    let fonts_dir = store.root().join("fonts");
    let widget_files_dir = store.root().join("widgets");
    let weather_path = store.root().join("weather.json");
    let script_approvals_path = store.root().join("script-approvals.json");

    // Fullscreen changes flow main -> service; the widget host's handle and
    // its state reports flow main -> service; shutdown flows main -> service;
    // "ready or failed" flows service -> main once.
    let (game_tx, game_rx) = mpsc::unbounded_channel::<Option<String>>();
    let (host_tx, host_rx) = mpsc::unbounded_channel::<FromHost>();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    // The tray menu reads the preset list from here; the service keeps it current.
    let presets = Arc::new(Mutex::new(Vec::new()));
    let preset_mirror = presets.clone();
    let service_thread =
        thread::Builder::new().name("service".into()).spawn(move || run_service(store, game_rx, host_rx, shutdown_rx, ready_tx, preset_mirror)).expect("the service thread starts");

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            error!("{message}");
            eprintln!("lumen-core: {message}");
            return ExitCode::FAILURE;
        }
        Err(_) => return ExitCode::FAILURE,
    }

    let tray_tx = host_tx.clone();
    let controls = tray::Controls {
        presets,
        apply_preset: Box::new(move |id| {
            let _ = tray_tx.send(FromHost::ApplyPreset(id));
        }),
    };
    let tray = match tray::Tray::new(controls) {
        Ok(tray) => Some(tray),
        Err(message) => {
            // Still useful without an icon; the settings app can drive it.
            error!("no tray icon: {message}");
            None
        }
    };

    // The desktop widgets live on this thread. The service sends the real
    // theme as soon as it has the handle; until then they use the fallback look.
    let options = host::Options { layout_path, bar_path, flyouts_path, launcher_path, styles_path, fonts_dir, widget_files_dir, weather_path, script_approvals_path, topmost: std::env::var_os("LUMEN_WIDGETS_TOPMOST").is_some() };
    let state_tx = host_tx.clone();
    let on_state = move |report| {
        let _ = state_tx.send(match report {
            host::Report::State(state) => FromHost::State(Box::new(state)),
            host::Report::Bar(config) => FromHost::Bar(Box::new(config)),
            host::Report::Flyouts(config) => FromHost::Flyouts(config),
            host::Report::Launcher(config) => FromHost::Launcher(config),
            host::Report::Styles(sheet) => FromHost::Styles(Box::new(sheet)),
            host::Report::Files(files) => FromHost::Files(files),
            host::Report::Weather(state) => FromHost::Weather(Box::new(state)),
            host::Report::Scripts(scripts) => FromHost::Scripts(scripts),
            host::Report::Costs(costs) => FromHost::Costs(costs),
        });
    };
    match host::start(&lumen_theme::Theme::fallback(), options, on_state) {
        Ok(handle) => {
            let _ = host_tx.send(FromHost::Ready(handle));
        }
        // The rest of Lumen still works without widgets.
        Err(message) => error!("no desktop widgets: {message}"),
    }

    // Blocks here, pumping this thread's messages (tray and widgets included), until Quit.
    let watching = lumen_win::fullscreen::watch(None, move |app| {
        // Nothing renders while a fullscreen app or game is in front: the
        // widgets' windows and graphics device are released...
        host::set_paused(app.is_some());
        let _ = game_tx.send(app.map(|app| app.title.clone()));
        if app.is_some() {
            // ...and the memory Lumen is not using goes back to Windows.
            lumen_win::trim_memory();
        }
    });
    if let Err(error) = watching {
        error!("fullscreen watcher: {error}");
    }

    host::stop();
    drop(tray);
    let _ = shutdown_tx.send(());
    let _ = service_thread.join();
    info!("lumen-core stopped");
    ExitCode::SUCCESS
}

/// What the widget host (main thread) sends to the service.
enum FromHost {
    /// The widgets are up; here is how to reach them.
    Ready(HostHandle),
    /// The layout, selection or edit mode changed.
    State(Box<lumen_ipc::WidgetsState>),
    /// The governor measured what each widget costs.
    Costs(Vec<lumen_ipc::WidgetCost>),
    /// The bar's settings (at start-up, and whenever they change).
    Bar(Box<lumen_ipc::BarConfig>),
    /// The flyout settings (at start-up, and whenever they change).
    Flyouts(lumen_ipc::FlyoutConfig),
    /// The launcher's settings (at start-up, and whenever they change).
    Launcher(lumen_ipc::LauncherConfig),
    /// The user's component styles (at start-up, and whenever they change).
    Styles(Box<lumen_ipc::StyleSheet>),
    /// The widget files, as read from the widgets folder (at start-up, and whenever one changes).
    Files(Vec<lumen_ipc::WidgetFileInfo>),
    /// The weather settings and forecast (at start-up, and whenever they change).
    Weather(Box<lumen_ipc::WeatherState>),
    /// The scripts widget files name, and whether each may run.
    Scripts(Vec<lumen_ipc::ScriptInfo>),
    /// A preset was picked from the tray menu.
    ApplyPreset(String),
}

fn run_service(
    store: Store,
    mut game_rx: mpsc::UnboundedReceiver<Option<String>>,
    mut host_rx: mpsc::UnboundedReceiver<FromHost>,
    shutdown_rx: oneshot::Receiver<()>,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
    preset_mirror: Arc<Mutex<Vec<lumen_ipc::PresetInfo>>>,
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
        // Write the kept theme out for other apps, in case it changed while Lumen was not running.
        service.borrow_mut().set_terminal_fragments(exports::terminal_fragments_dir());
        // A copy run against a test profile is never offered "start with Windows".
        if std::env::var_os("LUMEN_DATA_DIR").is_none()
            && let Ok(program) = std::env::current_exe()
        {
            service.borrow_mut().set_startup_entry(lumen_win::startup::RUN_KEY.to_owned(), program);
        }
        service.borrow_mut().set_preset_mirror(preset_mirror);
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

        let widget_service = service.clone();
        tokio::task::spawn_local(async move {
            while let Some(message) = host_rx.recv().await {
                match message {
                    FromHost::Ready(widgets) => widget_service.borrow_mut().set_widget_host(move |command| widgets.send(command)),
                    FromHost::State(state) => widget_service.borrow_mut().widgets_changed(*state),
                    FromHost::Costs(costs) => widget_service.borrow_mut().widget_costs_measured(costs),
                    FromHost::Bar(config) => widget_service.borrow_mut().bar_changed(*config),
                    FromHost::Flyouts(config) => widget_service.borrow_mut().flyouts_changed(config),
                    FromHost::Launcher(config) => widget_service.borrow_mut().launcher_changed(config),
                    FromHost::Styles(sheet) => widget_service.borrow_mut().styles_changed(*sheet),
                    FromHost::Files(files) => widget_service.borrow_mut().widget_files_changed(files),
                    FromHost::Weather(state) => widget_service.borrow_mut().weather_changed(*state),
                    FromHost::Scripts(scripts) => widget_service.borrow_mut().scripts_changed(scripts),
                    FromHost::ApplyPreset(id) => {
                        let params = serde_json::json!({ "id": id });
                        if let Err(error) = widget_service.borrow_mut().handle(lumen_ipc::method::APPLY_PRESET, params) {
                            error!("could not apply the preset from the tray: {}", error.message);
                        }
                    }
                }
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
    let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };

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
