//! The core's state and the IPC methods that act on it.
//!
//! A *preview* puts a theme on the real desktop without keeping it. The
//! desktop is snapshotted before the first preview; `commitPreview` keeps the
//! theme, `revertPreview` (or 15 seconds passing) restores the snapshot.

mod presets;

use std::{
    path::Path,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use lumen_ipc::{
    BarConfig, EditModeParams, FlyoutConfig, LauncherConfig, GameModeChanged, ImportWallpaperParams, Notification, PREVIEW_SECONDS, PresetInfo, PreviewState, PreviewThemeParams, RpcError, SelectWidgetParams,
    SetWallpaperParams, Status, ThemeChangeReason, ThemeChanged, ThemeState, UpdateWallpaperParams, WidgetCost, WidgetIdParams, WidgetsState, code, event, method,
};
use lumen_theme::{Mode, Source, SourceColors, Theme, Variant};
use lumen_widgets::host::Command as WidgetCommand;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::{
    sync::{Notify, broadcast},
    time::Instant,
};
use tracing::{info, warn};

use crate::{
    desktop::{ApplyPlan, Desktop},
    stats::ProcessStats,
    store::{Store, StoreError},
};

const PREVIEW_TIME: Duration = Duration::from_secs(PREVIEW_SECONDS);

pub struct Service<D: Desktop> {
    store: Store,
    desktop: D,
    /// The kept theme.
    committed: Option<Theme>,
    preview: Option<Preview<D::Snapshot>>,
    game: Option<String>,
    /// Serialised notifications, fanned out to every connected client.
    events: broadcast::Sender<Rc<str>>,
    /// Wakes the auto-revert task when the preview deadline changes.
    deadline_changed: Rc<Notify>,
    stats: ProcessStats,
    /// Sends commands to the widget host, once it is running.
    widget_host: Option<Box<dyn Fn(WidgetCommand)>>,
    /// The widget host's latest report.
    widgets: Option<WidgetsState>,
    /// The governor's latest per-widget measurements.
    widget_costs: Vec<WidgetCost>,
    /// The bar's settings, as the widget host last reported them.
    bar: Option<BarConfig>,
    /// The flyout settings, as the widget host last reported them.
    flyouts: Option<FlyoutConfig>,
    /// The launcher's settings, as the widget host last reported them.
    launcher: Option<LauncherConfig>,
    /// A copy of the preset list for the tray menu, which lives on another thread.
    preset_mirror: Option<Arc<Mutex<Vec<PresetInfo>>>>,
}

struct Preview<S> {
    theme: Theme,
    /// The desktop as it was before the first preview in this run of previews.
    snapshot: S,
    deadline: Instant,
}

impl<D: Desktop> Service<D> {
    pub fn new(store: Store, desktop: D) -> Self {
        let committed = store.active_theme();
        Self {
            store,
            desktop,
            committed,
            preview: None,
            game: None,
            events: broadcast::channel(64).0,
            deadline_changed: Rc::new(Notify::new()),
            stats: ProcessStats::new(),
            widget_host: None,
            widgets: None,
            widget_costs: Vec::new(),
            bar: None,
            flyouts: None,
            launcher: None,
            preset_mirror: None,
        }
    }

    /// Keeps `mirror` up to date with the preset list (for the tray menu).
    pub fn set_preset_mirror(&mut self, mirror: Arc<Mutex<Vec<PresetInfo>>>) {
        *mirror.lock().unwrap_or_else(|p| p.into_inner()) = self.preset_list();
        self.preset_mirror = Some(mirror);
    }

    /// A stream of notification lines for one client.
    pub fn subscribe(&self) -> broadcast::Receiver<Rc<str>> {
        self.events.subscribe()
    }

    pub fn deadline_changed(&self) -> Rc<Notify> {
        self.deadline_changed.clone()
    }

    /// When the current preview reverts by itself, if there is one.
    pub fn preview_deadline(&self) -> Option<Instant> {
        self.preview.as_ref().map(|p| p.deadline)
    }

    /// Handles one JSON-RPC request.
    pub fn handle(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            method::GET_THEME => json(self.theme_state()),
            method::PREVIEW_THEME => json(self.preview_theme(parse(params)?)?),
            method::COMMIT_PREVIEW => json(self.commit_preview()?),
            method::REVERT_PREVIEW => {
                self.revert_preview(ThemeChangeReason::Revert)?;
                json(self.theme_state())
            }
            method::LIST_WALLPAPERS => json(self.store.wallpapers()),
            method::IMPORT_WALLPAPER => {
                let params: ImportWallpaperParams = parse(params)?;
                json(self.store.import_wallpaper(Path::new(&params.path)).map_err(store_error)?)
            }
            method::GET_STATUS => json(self.status()),
            method::LIST_MONITORS => json(self.desktop.monitors().map_err(desktop_error)?),
            method::SET_WALLPAPER => {
                let params: SetWallpaperParams = parse(params)?;
                let image = self.store.resolve(&params.file);
                if !image.is_file() {
                    return Err(RpcError::new(code::NOT_FOUND, format!("no such wallpaper file: {}", image.display())));
                }
                json(self.desktop.set_wallpaper(params.monitor.as_deref(), &image).map_err(desktop_error)?)
            }
            method::UPDATE_WALLPAPER => {
                let params: UpdateWallpaperParams = parse(params)?;
                json(self.store.update_wallpaper(&params.id, params.name, params.favourite, params.tags).map_err(store_error)?)
            }
            method::GET_WIDGETS => self.widget_request(None),
            method::UPDATE_WIDGET => self.widget_request(Some(WidgetCommand::Update(parse(params)?))),
            method::ADD_WIDGET => self.widget_request(Some(WidgetCommand::Add(parse(params)?))),
            method::REMOVE_WIDGET => self.widget_request(Some(WidgetCommand::Remove(parse::<WidgetIdParams>(params)?.id))),
            method::SELECT_WIDGET => self.widget_request(Some(WidgetCommand::Select(parse::<SelectWidgetParams>(params)?.id))),
            method::SET_LAYOUT_OPTIONS => self.widget_request(Some(WidgetCommand::Options(parse(params)?))),
            method::SET_EDIT_MODE => self.widget_request(Some(WidgetCommand::EditMode(parse::<EditModeParams>(params)?.on))),
            method::RESUME_WIDGET => self.widget_request(Some(WidgetCommand::Resume(parse::<WidgetIdParams>(params)?.id))),
            method::GET_WIDGET_COSTS => json(&self.widget_costs),
            method::GET_BAR => json(self.bar.clone().unwrap_or_default()),
            method::SET_BAR => {
                let config = parse::<BarConfig>(params)?.sanitised();
                let host = self.widget_host.as_ref().ok_or_else(|| RpcError::new(code::WIDGETS_UNAVAILABLE, "the desktop widgets and bar are not running"))?;
                host(WidgetCommand::SetBar(Box::new(config.clone())));
                json(config)
            }
            method::GET_FLYOUTS => json(self.flyouts.clone().unwrap_or_default()),
            method::SET_FLYOUTS => {
                let config: FlyoutConfig = parse(params)?;
                let host = self.widget_host.as_ref().ok_or_else(|| RpcError::new(code::WIDGETS_UNAVAILABLE, "the desktop widgets and flyouts are not running"))?;
                host(WidgetCommand::SetFlyouts(Box::new(config.clone())));
                json(config)
            }
            method::SHOW_FLYOUT => {
                let host = self.widget_host.as_ref().ok_or_else(|| RpcError::new(code::WIDGETS_UNAVAILABLE, "the desktop widgets and flyouts are not running"))?;
                host(WidgetCommand::ShowFlyout);
                Ok(Value::Null)
            }
            method::GET_LAUNCHER => json(self.launcher.clone().unwrap_or_default()),
            method::SET_LAUNCHER => {
                let config: LauncherConfig = parse(params)?;
                let host = self.widget_host.as_ref().ok_or_else(|| RpcError::new(code::WIDGETS_UNAVAILABLE, "the desktop widgets and launcher are not running"))?;
                host(WidgetCommand::SetLauncher(config.clone()));
                json(config)
            }
            other => self.handle_preset(other, params).unwrap_or_else(|| Err(RpcError::new(code::METHOD_NOT_FOUND, format!("unknown method \"{other}\"")))),
        }
    }

    fn theme_state(&self) -> ThemeState {
        ThemeState {
            theme: self.committed.clone(),
            preview: self
                .preview
                .as_ref()
                .map(|p| PreviewState { theme: p.theme.clone(), seconds_left: p.deadline.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64 }),
        }
    }

    fn status(&mut self) -> Status {
        let (memory_mb, cpu_percent) = self.stats.sample();
        Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            data_dir: self.store.root().display().to_string(),
            memory_mb,
            cpu_percent,
            game_mode: GameModeChanged { active: self.game.is_some(), app: self.game.clone() },
            previewing: self.preview.is_some(),
        }
    }

    fn preview_theme(&mut self, mut params: PreviewThemeParams) -> Result<PreviewState, RpcError> {
        let only_monitor = params.wallpaper_monitor.take();
        let theme = self.build_theme(params)?;
        // "Show on monitor N": the source wallpaper goes on that monitor alone.
        let wallpapers = match (&theme.source, only_monitor) {
            (Source::Wallpaper { path }, Some(monitor)) => Some(vec![(Some(monitor), self.store.resolve(path))]),
            _ => None,
        };
        let plan = self.plan(&theme, wallpapers);

        // Previewing again while a preview is up keeps the original snapshot,
        // so Revert always returns to the look from before any of them.
        let (snapshot, fresh) = match self.preview.take() {
            Some(previous) => (previous.snapshot, false),
            None => (self.desktop.snapshot().map_err(desktop_error)?, true),
        };
        if let Err(error) = self.desktop.apply(&plan) {
            // Leave nothing half-applied.
            if let Err(restore_error) = self.desktop.restore(&snapshot) {
                warn!("could not undo a failed preview: {restore_error}");
            }
            if !fresh {
                self.emit_theme(None, ThemeChangeReason::Revert);
            }
            self.deadline_changed.notify_one();
            return Err(desktop_error(error));
        }

        info!("previewing \"{}\" for {PREVIEW_SECONDS} s", theme.name);
        self.preview = Some(Preview { theme: theme.clone(), snapshot, deadline: Instant::now() + PREVIEW_TIME });
        self.deadline_changed.notify_one();
        self.emit_theme(Some(&theme), ThemeChangeReason::Preview);
        Ok(PreviewState { theme, seconds_left: PREVIEW_SECONDS })
    }

    fn commit_preview(&mut self) -> Result<Theme, RpcError> {
        let Some(preview) = self.preview.take() else {
            return Err(RpcError::new(code::NO_PREVIEW, "there is no preview to keep (it may have already reverted)"));
        };
        self.deadline_changed.notify_one();
        if let Err(error) = self.store.save_active_theme(&preview.theme) {
            // Could not save: stay in preview so the user can retry or revert.
            self.preview = Some(preview);
            return Err(store_error(error));
        }
        info!("kept \"{}\"", preview.theme.name);
        self.committed = Some(preview.theme.clone());
        // A theme kept by hand is no longer "the preset as saved".
        self.look_changed_by_hand();
        self.emit_theme(None, ThemeChangeReason::Commit);
        Ok(preview.theme)
    }

    /// Puts the desktop back as it was before the preview. Not an error if
    /// nothing is being previewed.
    fn revert_preview(&mut self, reason: ThemeChangeReason) -> Result<(), RpcError> {
        let Some(preview) = self.preview.take() else {
            return Ok(());
        };
        self.deadline_changed.notify_one();
        let restored = self.desktop.restore(&preview.snapshot);
        info!("reverted \"{}\" ({reason:?})", preview.theme.name);
        self.emit_theme(None, reason);
        restored.map_err(desktop_error)
    }

    /// Called by the timer task: reverts the preview if its time is up.
    pub fn revert_if_due(&mut self) {
        if self.preview.as_ref().is_some_and(|p| p.deadline <= Instant::now())
            && let Err(error) = self.revert_preview(ThemeChangeReason::AutoRevert)
        {
            warn!("auto-revert: {}", error.message);
        }
    }

    /// Records a fullscreen app appearing (`Some(title)`) or going away.
    pub fn set_game_mode(&mut self, app: Option<String>) {
        if app.is_some() != self.game.is_some() {
            info!("game mode {}", if app.is_some() { "on" } else { "off" });
        }
        if app != self.game {
            self.game = app;
            self.emit(event::GAME_MODE_CHANGED, GameModeChanged { active: self.game.is_some(), app: self.game.clone() });
        }
    }

    /// Before exit: never leave an unkept preview on the desktop.
    pub fn shutdown(&mut self) {
        if let Err(error) = self.revert_preview(ThemeChangeReason::Revert) {
            warn!("could not revert the preview on exit: {}", error.message);
        }
    }

    // ----- helpers -----

    /// Builds the theme a `previewTheme` request describes.
    fn build_theme(&self, params: PreviewThemeParams) -> Result<Theme, RpcError> {
        let invalid = |message: String| RpcError::new(code::INVALID_PARAMS, message);
        let current = self.preview.as_ref().map(|p| &p.theme).or(self.committed.as_ref());

        let mut theme = if let Some(theme) = params.theme {
            theme
        } else {
            let source = params
                .source
                .or_else(|| current.map(|t| t.source.clone()))
                .ok_or_else(|| invalid("give a \"source\" (wallpaper or colour) or a complete \"theme\"; there is no current theme to start from".into()))?;
            let variant = params.variant.or(current.map(|t| t.variant)).unwrap_or(Variant::Tonal);
            let mode = params.mode.or(current.map(|t| t.mode)).unwrap_or(Mode::Dark);
            let contrast = params.contrast.or(current.map(|t| t.contrast)).unwrap_or(0);

            let mut theme = match &source {
                Source::Wallpaper { path } => {
                    let entry = self.store.wallpaper_by_file(path);
                    let colors = match &entry {
                        // Imported wallpapers already know their colours: no image decode.
                        Some(entry) if !entry.palette.is_empty() => SourceColors { candidates: entry.palette.clone() },
                        _ => lumen_theme::extract(self.store.resolve(path)).map_err(|e| RpcError::new(code::NOT_FOUND, e.to_string()))?,
                    };
                    let mut theme = Theme::from_wallpaper_colors(path, &colors, variant, mode, contrast).map_err(|e| invalid(e.to_string()))?;
                    if let Some(entry) = entry {
                        theme.name = entry.name;
                    }
                    theme
                }
                Source::Color { value } => Theme::from_color(*value, variant, mode, contrast).map_err(|e| invalid(e.to_string()))?,
                Source::Manual => return Err(invalid("a manual theme has no source to generate from; send the complete \"theme\"".into())),
            };
            // Style choices carry over when only the colours are regenerated.
            if let Some(current) = current {
                theme.style = current.style.clone();
                theme.set_windows_accent = current.set_windows_accent;
            }
            theme
        };

        if let Some(style) = params.style {
            theme.style = style;
        }
        if let Some(set) = params.set_windows_accent {
            theme.set_windows_accent = set;
        }
        if let Some(name) = params.name.filter(|n| !n.trim().is_empty()) {
            theme.name = name;
        }
        if !(-1..=1).contains(&theme.contrast) {
            return Err(invalid(format!("contrast must be -1, 0 or 1, got {}", theme.contrast)));
        }
        Ok(theme)
    }

    /// What a theme means for the desktop. `wallpapers` overrides the theme's
    /// own source wallpaper (presets carry one per monitor).
    fn plan(&self, theme: &Theme, wallpapers: Option<Vec<(Option<String>, std::path::PathBuf)>>) -> ApplyPlan {
        let wallpapers = wallpapers.unwrap_or_else(|| match &theme.source {
            Source::Wallpaper { path } => vec![(None, self.store.resolve(path))],
            _ => Vec::new(),
        });
        ApplyPlan { wallpapers, mode: theme.mode, accent: theme.set_windows_accent.then_some(theme.roles.primary) }
    }

    /// Announces the theme surfaces should show now: `previewed`, or the kept one.
    fn emit_theme(&self, previewed: Option<&Theme>, reason: ThemeChangeReason) {
        let theme = previewed.cloned().or_else(|| self.committed.clone());
        // Lumen's own surfaces (the desktop widgets) recolour first.
        if let Some(widgets) = &self.widget_host {
            widgets(WidgetCommand::Theme(Box::new(theme.clone().unwrap_or_else(Theme::fallback))));
        }
        self.emit(event::THEME_CHANGED, ThemeChanged { theme, preview: previewed.is_some(), reason });
    }

    /// Connects the widget host: `send` delivers commands to it. It is told
    /// the theme that is showing right now.
    pub fn set_widget_host(&mut self, send: impl Fn(WidgetCommand) + 'static) {
        let shown = self.preview.as_ref().map(|p| p.theme.clone()).or_else(|| self.committed.clone());
        send(WidgetCommand::Theme(Box::new(shown.unwrap_or_else(Theme::fallback))));
        self.widget_host = Some(Box::new(send));
    }

    /// The widget host reports a change to the layout, selection or edit mode.
    pub fn widgets_changed(&mut self, state: WidgetsState) {
        if self.widgets.as_ref() != Some(&state) {
            self.emit(event::WIDGETS_CHANGED, &state);
            self.widgets = Some(state);
        }
    }

    /// The widget host reports the launcher's settings (at start-up and on every change).
    pub fn launcher_changed(&mut self, config: LauncherConfig) {
        if self.launcher.as_ref() != Some(&config) {
            if self.launcher.is_some() {
                self.emit(event::LAUNCHER_CHANGED, &config);
            }
            self.launcher = Some(config);
        }
    }

    /// The widget host reports the flyout settings (at start-up and on every change).
    pub fn flyouts_changed(&mut self, config: FlyoutConfig) {
        if self.flyouts.as_ref() != Some(&config) {
            if self.flyouts.is_some() {
                self.emit(event::FLYOUTS_CHANGED, &config);
            }
            self.flyouts = Some(config);
        }
    }

    /// The widget host reports the bar's settings (at start-up and on every change).
    pub fn bar_changed(&mut self, config: BarConfig) {
        if self.bar.as_ref() != Some(&config) {
            // The first report is just the host saying what it loaded.
            if self.bar.is_some() {
                self.emit(event::BAR_CHANGED, &config);
            }
            self.bar = Some(config);
        }
    }

    /// The widget host reports what each widget cost over the last window.
    pub fn widget_costs_measured(&mut self, costs: Vec<WidgetCost>) {
        self.widget_costs = costs;
    }

    /// Passes a widget request on to the host. The answer is the state as it
    /// stands; the change itself comes back through [`Service::widgets_changed`].
    fn widget_request(&mut self, command: Option<WidgetCommand>) -> Result<Value, RpcError> {
        let unavailable = || RpcError::new(code::WIDGETS_UNAVAILABLE, "the desktop widgets are not running");
        let host = self.widget_host.as_ref().ok_or_else(unavailable)?;
        if let Some(command) = command {
            host(command);
        }
        json(self.widgets.as_ref().ok_or_else(unavailable)?)
    }

    fn emit(&self, name: &str, params: impl Serialize) {
        if let Ok(line) = serde_json::to_string(&Notification::new(name, params)) {
            // An error only means no client is connected right now.
            let _ = self.events.send(line.into());
        }
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    // Methods whose params are all optional may be called with none at all.
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::new(code::INVALID_PARAMS, format!("invalid params: {e}")))
}

fn json(value: impl Serialize) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(code::INTERNAL_ERROR, e.to_string()))
}

fn store_error(error: StoreError) -> RpcError {
    let code = match error {
        StoreError::NotFound(_) => code::NOT_FOUND,
        StoreError::Invalid(_) => code::INVALID_PARAMS,
        StoreError::Io(_) => code::INTERNAL_ERROR,
    };
    RpcError::new(code, error.to_string())
}

fn desktop_error(message: String) -> RpcError {
    RpcError::new(code::DESKTOP_FAILED, message)
}

/// Runs forever: reverts the preview when its 15 seconds are up.
pub async fn auto_revert<D: Desktop>(service: Rc<std::cell::RefCell<Service<D>>>) {
    let changed = service.borrow().deadline_changed();
    loop {
        let deadline = service.borrow().preview_deadline();
        match deadline {
            None => changed.notified().await,
            Some(deadline) => tokio::select! {
                () = tokio::time::sleep_until(deadline) => service.borrow_mut().revert_if_due(),
                () = changed.notified() => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, fs, path::PathBuf};

    use lumen_ipc::{MonitorInfo, WallpaperEntry};
    use lumen_theme::Color;
    use serde_json::json;
    use tokio::task::LocalSet;

    use super::*;
    use crate::store::tests::{temp_store, test_image};

    /// A pretend desktop that just remembers what it was told.
    #[derive(Default)]
    struct FakeDesktop {
        /// The "look" currently on the desktop.
        current: Option<ApplyPlan>,
        applied: Vec<ApplyPlan>,
        restores: usize,
        fail_apply: bool,
        /// Calls to `set_wallpaper`.
        wallpaper_only: Vec<(Option<String>, PathBuf)>,
        /// The monitors to report; one plain monitor if not set.
        monitor_list: Option<Vec<MonitorInfo>>,
    }

    fn fake_monitor(number: usize, id: &str, wallpaper: Option<&Path>) -> MonitorInfo {
        MonitorInfo { number, id: id.into(), x: 0, y: 0, width: 1920, height: 1080, primary: number == 1, wallpaper: wallpaper.map(|p| p.display().to_string()) }
    }

    impl Desktop for FakeDesktop {
        type Snapshot = Option<ApplyPlan>;

        fn snapshot(&mut self) -> Result<Self::Snapshot, String> {
            Ok(self.current.clone())
        }

        fn apply(&mut self, plan: &ApplyPlan) -> Result<(), String> {
            if self.fail_apply {
                return Err("Windows said no".into());
            }
            self.current = Some(plan.clone());
            self.applied.push(plan.clone());
            Ok(())
        }

        fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), String> {
            self.current = snapshot.clone();
            self.restores += 1;
            Ok(())
        }

        fn monitors(&mut self) -> Result<Vec<MonitorInfo>, String> {
            Ok(self.monitor_list.clone().unwrap_or_else(|| vec![fake_monitor(1, "MONITOR-A", None)]))
        }

        fn set_wallpaper(&mut self, monitor: Option<&str>, image: &Path) -> Result<(), String> {
            self.wallpaper_only.push((monitor.map(str::to_owned), image.to_owned()));
            Ok(())
        }
    }

    fn service() -> Service<FakeDesktop> {
        Service::new(temp_store(), FakeDesktop::default())
    }

    fn call<T: DeserializeOwned>(service: &mut Service<FakeDesktop>, method: &str, params: Value) -> T {
        serde_json::from_value(service.handle(method, params).unwrap_or_else(|e| panic!("{method} failed: {}", e.message))).unwrap()
    }

    fn events(rx: &mut broadcast::Receiver<Rc<str>>) -> Vec<Value> {
        std::iter::from_fn(|| rx.try_recv().ok()).map(|line| serde_json::from_str(&line).unwrap()).collect()
    }

    const BLUE: &str = "#2F5BEA";

    fn preview_blue(service: &mut Service<FakeDesktop>) -> PreviewState {
        call(service, method::PREVIEW_THEME, json!({ "source": { "type": "color", "value": BLUE }, "mode": "light" }))
    }

    #[tokio::test(start_paused = true)]
    async fn a_fresh_install_has_no_theme() {
        let state: ThemeState = call(&mut service(), method::GET_THEME, Value::Null);
        assert_eq!(state, ThemeState { theme: None, preview: None });
    }

    #[tokio::test(start_paused = true)]
    async fn preview_applies_to_the_desktop_without_keeping() {
        let mut service = service();
        let mut rx = service.subscribe();
        let preview = preview_blue(&mut service);

        assert_eq!(preview.seconds_left, 15);
        assert_eq!(preview.theme.mode, Mode::Light);
        let plan = service.desktop.current.clone().unwrap();
        assert_eq!(plan.mode, Mode::Light);
        assert_eq!(plan.accent, Some(preview.theme.roles.primary));
        assert!(plan.wallpapers.is_empty(), "a colour theme does not touch the wallpaper");

        let state: ThemeState = call(&mut service, method::GET_THEME, Value::Null);
        assert_eq!(state.theme, None, "not kept yet");
        assert_eq!(state.preview.unwrap().theme, preview.theme);
        assert!(service.store.active_theme().is_none(), "nothing written to disk");

        let events = events(&mut rx);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["method"], "themeChanged");
        assert_eq!(events[0]["params"]["preview"], true);
        assert_eq!(events[0]["params"]["reason"], "preview");
        assert_eq!(events[0]["params"]["theme"]["roles"]["primary"], json!(preview.theme.roles.primary));
    }

    #[tokio::test(start_paused = true)]
    async fn preview_auto_reverts_after_15_seconds() {
        let service = Rc::new(RefCell::new(service()));
        let mut rx = service.borrow().subscribe();
        let local = LocalSet::new();
        local.spawn_local(auto_revert(service.clone()));
        local
            .run_until(async {
                preview_blue(&mut service.borrow_mut());
                events(&mut rx);

                tokio::time::sleep(Duration::from_secs(14)).await;
                assert!(service.borrow().preview.is_some(), "still previewing at 14 s");
                assert_eq!(service.borrow().desktop.restores, 0);
                let state: ThemeState = call(&mut service.borrow_mut(), method::GET_THEME, Value::Null);
                assert_eq!(state.preview.unwrap().seconds_left, 1);

                tokio::time::sleep(Duration::from_secs(2)).await;
                assert!(service.borrow().preview.is_none(), "reverted by 16 s");
                assert_eq!(service.borrow().desktop.restores, 1);
                assert_eq!(service.borrow().desktop.current, None, "desktop is back to how it started");

                let events = events(&mut rx);
                assert_eq!(events.len(), 1);
                assert_eq!(events[0]["params"]["reason"], "autoRevert");
                assert_eq!(events[0]["params"]["preview"], false);
                assert_eq!(events[0]["params"]["theme"], Value::Null);
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_preview_restarts_the_15_seconds() {
        let service = Rc::new(RefCell::new(service()));
        let local = LocalSet::new();
        local.spawn_local(auto_revert(service.clone()));
        local
            .run_until(async {
                preview_blue(&mut service.borrow_mut());
                tokio::time::sleep(Duration::from_secs(10)).await;
                let _: PreviewState = call(&mut service.borrow_mut(), method::PREVIEW_THEME, json!({ "variant": "vibrant" }));

                tokio::time::sleep(Duration::from_secs(10)).await;
                assert!(service.borrow().preview.is_some(), "20 s after the first preview, 10 s after the second");
                tokio::time::sleep(Duration::from_secs(6)).await;
                assert!(service.borrow().preview.is_none());
                assert_eq!(service.borrow().desktop.current, None, "reverts to before the *first* preview");
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn commit_keeps_the_theme_and_stops_the_timer() {
        let service = Rc::new(RefCell::new(service()));
        let mut rx = service.borrow().subscribe();
        let local = LocalSet::new();
        local.spawn_local(auto_revert(service.clone()));
        local
            .run_until(async {
                let preview = preview_blue(&mut service.borrow_mut());
                let kept: Theme = call(&mut service.borrow_mut(), method::COMMIT_PREVIEW, Value::Null);
                assert_eq!(kept, preview.theme);

                tokio::time::sleep(Duration::from_secs(60)).await;
                let service = service.borrow();
                assert_eq!(service.desktop.restores, 0, "a kept theme never reverts");
                assert_eq!(service.committed.as_ref(), Some(&kept));
                assert_eq!(service.store.active_theme(), Some(kept), "and it is on disk");

                let reasons: Vec<_> = events(&mut rx).iter().map(|e| e["params"]["reason"].clone()).collect();
                assert_eq!(reasons, [json!("preview"), json!("commit")]);
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_kept_theme_is_loaded_on_the_next_start() {
        let mut first = service();
        preview_blue(&mut first);
        let kept: Theme = call(&mut first, method::COMMIT_PREVIEW, Value::Null);

        let mut second = Service::new(Store::open(first.store.root()).unwrap(), FakeDesktop::default());
        let state: ThemeState = call(&mut second, method::GET_THEME, Value::Null);
        assert_eq!(state.theme, Some(kept));
        assert!(second.desktop.applied.is_empty(), "starting up does not touch the desktop");
    }

    #[tokio::test(start_paused = true)]
    async fn manual_revert_restores_the_kept_theme() {
        let mut service = service();
        preview_blue(&mut service);
        let kept: Theme = call(&mut service, method::COMMIT_PREVIEW, Value::Null);
        let after_commit = service.desktop.current.clone();

        let _: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "mode": "dark", "variant": "expressive" }));
        assert_ne!(service.desktop.current, after_commit);
        let mut rx = service.subscribe();
        let state: ThemeState = call(&mut service, method::REVERT_PREVIEW, Value::Null);

        assert_eq!(state, ThemeState { theme: Some(kept.clone()), preview: None });
        assert_eq!(service.desktop.current, after_commit);
        let events = events(&mut rx);
        assert_eq!(events[0]["params"]["reason"], "revert");
        assert_eq!(events[0]["params"]["theme"]["name"], json!(kept.name));
    }

    #[tokio::test(start_paused = true)]
    async fn commit_without_a_preview_is_an_error_but_revert_is_not() {
        let mut service = service();
        let error = service.handle(method::COMMIT_PREVIEW, Value::Null).unwrap_err();
        assert_eq!(error.code, code::NO_PREVIEW);
        assert!(service.handle(method::REVERT_PREVIEW, Value::Null).is_ok());
        assert_eq!(service.desktop.restores, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn changing_one_field_regenerates_from_the_current_theme() {
        let mut service = service();
        let first = preview_blue(&mut service).theme;
        let second: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "variant": "vibrant", "setWindowsAccent": false }));

        assert_eq!(second.theme.source, first.source);
        assert_eq!(second.theme.mode, first.mode);
        assert_eq!(second.theme.variant, Variant::Vibrant);
        assert_ne!(second.theme.roles, first.roles);
        assert_eq!(service.desktop.current.as_ref().unwrap().accent, None, "accent left alone when the theme says so");
    }

    #[tokio::test(start_paused = true)]
    async fn imported_wallpaper_previews_from_its_stored_palette() {
        let mut service = service();
        let image = test_image(service.store.root(), "Forest Day.png", [40, 140, 70]);
        let entry: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": image }));
        let listed: Vec<WallpaperEntry> = call(&mut service, method::LIST_WALLPAPERS, Value::Null);
        assert_eq!(listed, vec![entry.clone()]);

        // Delete the pixels: generating must not need to read the image again.
        let bytes = fs::read(&entry.path).unwrap();
        fs::write(&entry.path, b"").unwrap();
        let preview: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "source": { "type": "wallpaper", "path": entry.file } }));
        fs::write(&entry.path, bytes).unwrap();

        assert_eq!(preview.theme.name, "Forest Day");
        assert_eq!(preview.theme.source, Source::Wallpaper { path: "wallpapers/forest-day.png".into() });
        let plan = service.desktop.current.clone().unwrap();
        assert_eq!(plan.wallpapers, vec![(None, PathBuf::from(&entry.path))], "wallpaper goes on every monitor");
    }

    #[tokio::test(start_paused = true)]
    async fn a_complete_theme_is_used_as_given() {
        let mut service = service();
        let mut theme = Theme::from_color(Color::new(1, 2, 3), Variant::Monochrome, Mode::Dark, 0).unwrap();
        theme.source = Source::Manual;
        theme.roles.primary = Color::new(0xAB, 0xCD, 0xEF);
        let preview: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "theme": theme, "name": "Hand Made" }));
        assert_eq!(preview.theme.roles.primary, Color::new(0xAB, 0xCD, 0xEF));
        assert_eq!(preview.theme.name, "Hand Made");
    }

    #[tokio::test(start_paused = true)]
    async fn bad_requests_get_clear_errors_and_change_nothing() {
        let mut service = service();
        let cases = [
            (method::PREVIEW_THEME, json!({}), code::INVALID_PARAMS),
            (method::PREVIEW_THEME, json!({ "variant": "neon" }), code::INVALID_PARAMS),
            (method::PREVIEW_THEME, json!({ "sauce": 1 }), code::INVALID_PARAMS),
            (method::PREVIEW_THEME, json!({ "source": { "type": "color", "value": BLUE }, "contrast": 7 }), code::INVALID_PARAMS),
            (method::PREVIEW_THEME, json!({ "source": { "type": "wallpaper", "path": "wallpapers/missing.jpg" } }), code::NOT_FOUND),
            (method::IMPORT_WALLPAPER, json!({ "path": "C:/nope/missing.png" }), code::NOT_FOUND),
            (method::IMPORT_WALLPAPER, json!({}), code::INVALID_PARAMS),
            (method::APPLY_PRESET, json!({ "id": "missing" }), code::NOT_FOUND),
            (method::APPLY_PRESET, json!({ "id": "../config" }), code::INVALID_PARAMS),
            ("formatDisk", Value::Null, code::METHOD_NOT_FOUND),
        ];
        for (method, params, expected) in cases {
            let error = service.handle(method, params.clone()).unwrap_err();
            assert_eq!(error.code, expected, "{method} {params}: {}", error.message);
        }
        assert!(service.desktop.applied.is_empty());
        assert!(service.preview.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_apply_leaves_no_preview_behind() {
        let mut service = service();
        service.desktop.fail_apply = true;
        let error = service.handle(method::PREVIEW_THEME, json!({ "source": { "type": "color", "value": BLUE } })).unwrap_err();
        assert_eq!(error.code, code::DESKTOP_FAILED);
        assert!(service.preview.is_none());
        assert_eq!(service.desktop.restores, 1, "whatever was half-applied is undone");
    }

    #[tokio::test(start_paused = true)]
    async fn saving_a_preset_captures_the_whole_look() {
        let mut service = service();
        let root = service.store.root().to_owned();
        // Monitor 1 shows a library wallpaper; monitor 2 shows a picture from elsewhere on disk.
        let sky: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": test_image(&root, "sky.png", [40, 80, 200]) }));
        let outside = test_image(&root, "holiday.png", [200, 160, 40]);
        service.desktop.monitor_list = Some(vec![fake_monitor(1, "LEFT", Some(Path::new(&sky.path))), fake_monitor(2, "RIGHT", Some(&outside))]);
        // A widget sits on the second monitor.
        let mut state = WidgetsState::default();
        state.layout.widgets[1].monitor = Some("RIGHT".into());
        service.widgets_changed(state);

        let nothing = service.handle(method::SAVE_PRESET, json!({ "name": "Too Early" })).unwrap_err();
        assert_eq!(nothing.code, code::INVALID_PARAMS, "there is no theme to save yet");

        // Saving during a preview keeps that theme.
        let previewed = preview_blue(&mut service).theme;
        let mut rx = service.subscribe();
        let info: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "  Blue Monday " }));
        assert_eq!((info.id.as_str(), info.name.as_str(), info.active, info.widget_count), ("blue-monday", "Blue Monday", true, 3));
        assert!(service.preview.is_none());
        assert_eq!(service.committed.as_ref(), Some(&previewed));

        let saved = service.store.preset("blue-monday").unwrap();
        assert_eq!(saved.theme, previewed);
        assert_eq!(saved.wallpapers.iter().map(|w| (w.monitor, w.file.as_str())).collect::<Vec<_>>(), [(1, "wallpapers/sky.png"), (2, "wallpapers/holiday.png")]);
        assert_eq!(service.store.wallpapers().len(), 2, "the outside picture was copied into the library");
        let widgets = saved.widgets.unwrap();
        assert_eq!(widgets.widgets[1].monitor.as_deref(), Some("#2"), "monitors are stored by number");
        assert!(events(&mut rx).iter().any(|e| e["method"] == "presetsChanged" && e["params"][0]["name"] == "Blue Monday"));

        let listed: Vec<lumen_ipc::PresetInfo> = call(&mut service, method::LIST_PRESETS, Value::Null);
        assert_eq!(listed, vec![info]);
    }

    #[tokio::test(start_paused = true)]
    async fn applying_a_preset_restores_wallpapers_theme_and_widgets() {
        let mut service = service();
        let root = service.store.root().to_owned();
        let sky: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": test_image(&root, "sky.png", [40, 80, 200]) }));
        let ember: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": test_image(&root, "ember.png", [200, 80, 40]) }));
        service.desktop.monitor_list = Some(vec![fake_monitor(1, "LEFT", Some(Path::new(&sky.path))), fake_monitor(2, "RIGHT", Some(Path::new(&ember.path)))]);
        let mut state = WidgetsState::default();
        state.layout.widgets[0].x = 480.0;
        state.layout.widgets[2].monitor = Some("RIGHT".into());
        service.widgets_changed(state.clone());
        let sent: Rc<RefCell<Vec<WidgetCommand>>> = Rc::default();
        let log = sent.clone();
        service.set_widget_host(move |command| log.borrow_mut().push(command));

        preview_blue(&mut service);
        let _: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "First Look" }));
        let first_theme = service.committed.clone().unwrap();

        // Change everything: another theme kept by hand, on a PC whose monitors now have other ids.
        let _: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "source": { "type": "color", "value": "#C0392B" }, "mode": "dark" }));
        let _: Theme = call(&mut service, method::COMMIT_PREVIEW, Value::Null);
        assert!(!call::<Vec<lumen_ipc::PresetInfo>>(&mut service, method::LIST_PRESETS, Value::Null)[0].active, "the look was changed by hand");
        service.desktop.monitor_list = Some(vec![fake_monitor(1, "NEW-LEFT", None), fake_monitor(2, "NEW-RIGHT", None)]);
        sent.borrow_mut().clear();

        // A preview in progress is replaced, not left dangling.
        preview_blue(&mut service);
        let mut rx = service.subscribe();
        let applied: Theme = call(&mut service, method::APPLY_PRESET, json!({ "id": "first-look" }));

        assert_eq!(applied, first_theme);
        assert!(service.preview.is_none());
        assert_eq!(service.store.active_theme(), Some(first_theme));
        let plan = service.desktop.current.clone().unwrap();
        assert_eq!(plan.wallpapers, vec![(Some("NEW-LEFT".into()), PathBuf::from(&sky.path)), (Some("NEW-RIGHT".into()), PathBuf::from(&ember.path))]);
        let layout =
            sent.borrow().iter().find_map(|c| if let WidgetCommand::SetLayout(layout) = c { Some(layout.clone()) } else { None }).expect("the widget layout was sent to the host");
        assert_eq!(layout.widgets[0].x, 480.0);
        assert_eq!(layout.widgets[2].monitor.as_deref(), Some("NEW-RIGHT"), "monitor 2 on this PC");
        let reasons: Vec<_> = events(&mut rx).iter().filter(|e| e["method"] == "themeChanged").map(|e| e["params"]["reason"].clone()).collect();
        assert_eq!(reasons, [json!("revert"), json!("preset")]);
        let listed: Vec<lumen_ipc::PresetInfo> = call(&mut service, method::LIST_PRESETS, Value::Null);
        assert!(listed[0].active && listed[0].last_used_at > 0);
    }

    #[tokio::test(start_paused = true)]
    async fn presets_can_be_renamed_copied_exported_imported_and_deleted() {
        let mut service = service();
        let root = service.store.root().to_owned();
        let sky: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": test_image(&root, "sky.png", [40, 80, 200]) }));
        service.desktop.monitor_list = Some(vec![fake_monitor(1, "LEFT", Some(Path::new(&sky.path)))]);
        preview_blue(&mut service);
        let _: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "Sky" }));

        let renamed: lumen_ipc::PresetInfo = call(&mut service, method::RENAME_PRESET, json!({ "id": "sky", "name": "Big Sky" }));
        assert_eq!((renamed.id.as_str(), renamed.name.as_str()), ("sky", "Big Sky"), "the id stays, the name changes");
        let copy: lumen_ipc::PresetInfo = call(&mut service, method::DUPLICATE_PRESET, json!({ "id": "sky" }));
        assert_eq!((copy.id.as_str(), copy.name.as_str(), copy.active), ("big-sky-copy", "Big Sky copy", false));

        let file = root.join("exported");
        let _: Value = call(&mut service, method::EXPORT_PRESET, json!({ "id": "sky", "path": file }));
        assert!(root.join("exported.lumen").is_file(), "the extension is added");
        let imported: lumen_ipc::PresetInfo = call(&mut service, method::IMPORT_PRESET, json!({ "path": root.join("exported.lumen") }));
        assert_eq!(imported.name, "Big Sky");
        assert_ne!(imported.id, "sky");

        let _: Value = call(&mut service, method::DELETE_PRESET, json!({ "id": "sky" }));
        let names: Vec<String> = call::<Vec<lumen_ipc::PresetInfo>>(&mut service, method::LIST_PRESETS, Value::Null).into_iter().map(|p| p.name).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Big Sky copy".to_owned()) && names.contains(&"Big Sky".to_owned()));

        for (method, params, expected) in [
            (method::RENAME_PRESET, json!({ "id": "big-sky-copy", "name": "  " }), code::INVALID_PARAMS),
            (method::EXPORT_PRESET, json!({ "id": "big-sky-copy", "path": "relative.lumen" }), code::INVALID_PARAMS),
            (method::EXPORT_PRESET, json!({ "id": "gone", "path": root.join("x.lumen") }), code::NOT_FOUND),
            (method::IMPORT_PRESET, json!({ "path": root.join("nope.lumen") }), code::NOT_FOUND),
            (method::DELETE_PRESET, json!({ "id": "gone" }), code::NOT_FOUND),
        ] {
            assert_eq!(service.handle(method, params.clone()).unwrap_err().code, expected, "{method} {params}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn show_on_one_monitor_previews_the_wallpaper_there_only() {
        let mut service = service();
        let image = test_image(service.store.root(), "sky.png", [40, 80, 200]);
        let entry: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": image }));
        let params = json!({ "source": { "type": "wallpaper", "path": entry.file }, "wallpaperMonitor": "MONITOR-B" });
        let _: PreviewState = call(&mut service, method::PREVIEW_THEME, params);
        let plan = service.desktop.current.clone().unwrap();
        assert_eq!(plan.wallpapers, vec![(Some("MONITOR-B".into()), PathBuf::from(&entry.path))]);
    }

    #[tokio::test(start_paused = true)]
    async fn set_wallpaper_applies_at_once_without_a_preview() {
        let mut service = service();
        let image = test_image(service.store.root(), "sky.png", [40, 80, 200]);
        let entry: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": image }));
        let _: Value = call(&mut service, method::SET_WALLPAPER, json!({ "file": entry.file, "monitor": "MONITOR-A" }));

        assert_eq!(service.desktop.wallpaper_only, vec![(Some("MONITOR-A".into()), PathBuf::from(&entry.path))]);
        assert!(service.preview.is_none() && service.desktop.applied.is_empty());
        let missing = service.handle(method::SET_WALLPAPER, json!({ "file": "wallpapers/missing.jpg" })).unwrap_err();
        assert_eq!(missing.code, code::NOT_FOUND);
    }

    #[tokio::test(start_paused = true)]
    async fn status_monitors_and_favourites_are_served() {
        let mut service = service();
        let status: Status = call(&mut service, method::GET_STATUS, Value::Null);
        assert!(status.memory_mb > 0.0 && !status.previewing && !status.game_mode.active);
        assert_eq!(status.data_dir, service.store.root().display().to_string());
        preview_blue(&mut service);
        service.set_game_mode(Some("Some Game".into()));
        let status: Status = call(&mut service, method::GET_STATUS, Value::Null);
        assert!(status.previewing && status.game_mode.active);

        let monitors: Vec<MonitorInfo> = call(&mut service, method::LIST_MONITORS, Value::Null);
        assert_eq!(monitors.len(), 1);

        let image = test_image(service.store.root(), "sky.png", [40, 80, 200]);
        let entry: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": image }));
        let updated: WallpaperEntry = call(&mut service, method::UPDATE_WALLPAPER, json!({ "id": entry.id, "favourite": true, "tags": ["calm"] }));
        assert!(updated.favourite);
        assert_eq!(updated.tags, ["calm"]);
    }

    #[tokio::test(start_paused = true)]
    async fn widgets_are_told_every_theme_to_show() {
        let mut service = service();
        let seen: Rc<RefCell<Vec<String>>> = Rc::default();
        let log = seen.clone();
        service.set_widget_host(move |command| {
            if let WidgetCommand::Theme(theme) = command {
                log.borrow_mut().push(format!("{} {}", theme.name, theme.mode));
            }
        });
        assert_eq!(*seen.borrow(), ["Lumen dark"], "no theme yet: the fallback look");

        preview_blue(&mut service);
        let _: Theme = call(&mut service, method::COMMIT_PREVIEW, Value::Null);
        let _: PreviewState = call(&mut service, method::PREVIEW_THEME, json!({ "mode": "dark", "name": "Night" }));
        let _: ThemeState = call(&mut service, method::REVERT_PREVIEW, Value::Null);
        assert_eq!(*seen.borrow(), ["Lumen dark", "#2F5BEA light", "#2F5BEA light", "Night dark", "#2F5BEA light"], "preview, keep, preview, revert");
    }

    #[tokio::test(start_paused = true)]
    async fn widget_requests_go_to_the_host_and_its_reports_become_events() {
        let mut service = service();
        let unavailable = service.handle(method::GET_WIDGETS, Value::Null).unwrap_err();
        assert_eq!(unavailable.code, code::WIDGETS_UNAVAILABLE, "no host yet");

        let sent: Rc<RefCell<Vec<String>>> = Rc::default();
        let log = sent.clone();
        service.set_widget_host(move |command| log.borrow_mut().push(format!("{command:?}")));
        let mut rx = service.subscribe();
        let state = WidgetsState::default();
        service.widgets_changed(state.clone());
        service.widgets_changed(state.clone());
        assert_eq!(events(&mut rx).len(), 1, "an unchanged report is not announced twice");

        let got: WidgetsState = call(&mut service, method::GET_WIDGETS, Value::Null);
        assert_eq!(got, state);
        let _: WidgetsState = call(&mut service, method::UPDATE_WIDGET, json!({ "id": "clock-1", "x": 240, "y": 96, "dragging": true }));
        let _: WidgetsState = call(&mut service, method::ADD_WIDGET, json!({ "kind": "nowPlaying" }));
        let _: WidgetsState = call(&mut service, method::REMOVE_WIDGET, json!({ "id": "clock-1" }));
        let _: WidgetsState = call(&mut service, method::SELECT_WIDGET, json!({ "id": null }));
        let _: WidgetsState = call(&mut service, method::SET_LAYOUT_OPTIONS, json!({ "locked": true }));
        let _: WidgetsState = call(&mut service, method::SET_EDIT_MODE, json!({ "on": true }));

        let sent = sent.borrow();
        let kinds: Vec<&str> = sent.iter().map(|c| c.split(['(', ' ']).next().unwrap()).collect();
        assert_eq!(kinds, ["Theme", "Update", "Add", "Remove", "Select", "Options", "EditMode"]);
        assert!(sent[1].contains("x: Some(240.0)") && sent[1].contains("dragging: true"), "{}", sent[1]);
        assert!(sent[6].contains("true"));

        // The host's next report reaches clients as an event.
        let mut edited = state;
        edited.edit_mode = true;
        service.widgets_changed(edited);
        let events = events(&mut rx);
        assert_eq!(events[0]["method"], "widgetsChanged");
        assert_eq!(events[0]["params"]["editMode"], true);

        let bad = service.handle(method::UPDATE_WIDGET, json!({ "id": "clock-1", "colour": "red" })).unwrap_err();
        assert_eq!(bad.code, code::INVALID_PARAMS);
    }

    #[tokio::test(start_paused = true)]
    async fn the_bar_is_set_through_the_host_and_saved_in_presets() {
        let mut service = service();
        let wanted = json!({ "enabled": true, "position": "bottom", "height": 250, "monitor": "#2" });
        assert_eq!(service.handle(method::SET_BAR, wanted.clone()).unwrap_err().code, code::WIDGETS_UNAVAILABLE, "no host yet");
        let off: BarConfig = call(&mut service, method::GET_BAR, Value::Null);
        assert!(!off.enabled, "off until the host says otherwise");

        let sent: Rc<RefCell<Vec<WidgetCommand>>> = Rc::default();
        let log = sent.clone();
        service.set_widget_host(move |command| log.borrow_mut().push(command));
        let mut rx = service.subscribe();
        service.bar_changed(BarConfig::default());
        assert!(events(&mut rx).is_empty(), "the host's first report is what it loaded, not a change");

        let set: BarConfig = call(&mut service, method::SET_BAR, wanted);
        assert!(set.enabled && set.monitor == "#2");
        assert!(set.height < 250, "the height is kept sensible");
        let bars = || sent.borrow().iter().filter_map(|c| if let WidgetCommand::SetBar(bar) = c { Some((**bar).clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(bars(), std::slice::from_ref(&set));

        // The host applies it and reports back; clients hear once.
        service.bar_changed(set.clone());
        service.bar_changed(set.clone());
        let heard = events(&mut rx);
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0]["method"], "barChanged");
        assert_eq!(heard[0]["params"]["position"], "bottom");
        assert_eq!(call::<BarConfig>(&mut service, method::GET_BAR, Value::Null), set);

        // A preset carries the bar, and brings it back.
        preview_blue(&mut service);
        let _: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "With Bar" }));
        service.bar_changed(BarConfig::default());
        sent.borrow_mut().clear();
        let _: Theme = call(&mut service, method::APPLY_PRESET, json!({ "id": "with-bar" }));
        assert_eq!(bars(), [set]);
    }

    #[tokio::test(start_paused = true)]
    async fn flyout_settings_go_through_the_host_and_into_presets() {
        let mut service = service();
        let wanted = json!({ "volume": true, "style": "card", "position": "topRight" });
        assert_eq!(service.handle(method::SET_FLYOUTS, wanted.clone()).unwrap_err().code, code::WIDGETS_UNAVAILABLE, "no host yet");
        assert_eq!(service.handle(method::SHOW_FLYOUT, Value::Null).unwrap_err().code, code::WIDGETS_UNAVAILABLE);
        assert!(!call::<FlyoutConfig>(&mut service, method::GET_FLYOUTS, Value::Null).volume, "off until the host says otherwise");

        let sent: Rc<RefCell<Vec<WidgetCommand>>> = Rc::default();
        let log = sent.clone();
        service.set_widget_host(move |command| log.borrow_mut().push(command));
        let mut rx = service.subscribe();
        service.flyouts_changed(FlyoutConfig::default());
        assert!(events(&mut rx).is_empty(), "the host's first report is what it loaded, not a change");

        let set: FlyoutConfig = call(&mut service, method::SET_FLYOUTS, wanted);
        assert!(set.volume);
        let _: Value = call(&mut service, method::SHOW_FLYOUT, Value::Null);
        let flyouts = || sent.borrow().iter().filter_map(|c| if let WidgetCommand::SetFlyouts(f) = c { Some((**f).clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(flyouts(), std::slice::from_ref(&set));
        assert!(sent.borrow().iter().any(|c| matches!(c, WidgetCommand::ShowFlyout)));

        service.flyouts_changed(set.clone());
        service.flyouts_changed(set.clone());
        let heard = events(&mut rx);
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0]["method"], "flyoutsChanged");
        assert_eq!(heard[0]["params"]["position"], "topRight");

        // A preset carries the flyout settings, and brings them back.
        preview_blue(&mut service);
        let _: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "With Flyout" }));
        service.flyouts_changed(FlyoutConfig::default());
        sent.borrow_mut().clear();
        let _: Theme = call(&mut service, method::APPLY_PRESET, json!({ "id": "with-flyout" }));
        assert_eq!(flyouts(), [set]);
        assert!(service.handle(method::SET_FLYOUTS, json!({ "position": "middle" })).is_err(), "an unknown position is refused");
    }

    #[tokio::test(start_paused = true)]
    async fn launcher_settings_go_through_the_host_and_into_presets() {
        let mut service = service();
        let wanted = json!({ "enabled": true, "hotkey": "ctrlSpace" });
        assert_eq!(service.handle(method::SET_LAUNCHER, wanted.clone()).unwrap_err().code, code::WIDGETS_UNAVAILABLE, "no host yet");
        assert!(!call::<LauncherConfig>(&mut service, method::GET_LAUNCHER, Value::Null).enabled, "off until the host says otherwise");

        let sent: Rc<RefCell<Vec<WidgetCommand>>> = Rc::default();
        let log = sent.clone();
        service.set_widget_host(move |command| log.borrow_mut().push(command));
        let mut rx = service.subscribe();
        service.launcher_changed(LauncherConfig::default());
        assert!(events(&mut rx).is_empty(), "the host's first report is what it loaded, not a change");

        let set: LauncherConfig = call(&mut service, method::SET_LAUNCHER, wanted);
        assert!(set.enabled);
        let launchers = || sent.borrow().iter().filter_map(|c| if let WidgetCommand::SetLauncher(l) = c { Some(l.clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(launchers(), std::slice::from_ref(&set));

        service.launcher_changed(set.clone());
        service.launcher_changed(set.clone());
        let heard = events(&mut rx);
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0]["method"], "launcherChanged");
        assert_eq!(heard[0]["params"]["hotkey"], "ctrlSpace");

        // A preset carries the launcher's settings, and brings them back.
        preview_blue(&mut service);
        let _: lumen_ipc::PresetInfo = call(&mut service, method::SAVE_PRESET, json!({ "name": "With Launcher" }));
        service.launcher_changed(LauncherConfig::default());
        sent.borrow_mut().clear();
        let _: Theme = call(&mut service, method::APPLY_PRESET, json!({ "id": "with-launcher" }));
        assert_eq!(launchers(), [set]);
        assert!(service.handle(method::SET_LAUNCHER, json!({ "hotkey": "f13" })).is_err(), "an unknown hotkey is refused");
    }

    #[tokio::test(start_paused = true)]
    async fn game_mode_changes_are_announced_once() {
        let mut service = service();
        let mut rx = service.subscribe();
        service.set_game_mode(Some("Some Game".into()));
        service.set_game_mode(Some("Some Game".into()));
        service.set_game_mode(None);

        let events = events(&mut rx);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], json!({ "jsonrpc": "2.0", "method": "gameModeChanged", "params": { "active": true, "app": "Some Game" } }));
        assert_eq!(events[1]["params"], json!({ "active": false, "app": null }));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_reverts_an_unkept_preview() {
        let mut service = service();
        preview_blue(&mut service);
        service.shutdown();
        assert_eq!(service.desktop.restores, 1);
        assert_eq!(service.desktop.current, None);
    }
}
