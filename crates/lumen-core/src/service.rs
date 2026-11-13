//! The core's state and the IPC methods that act on it.
//!
//! A *preview* puts a theme on the real desktop without keeping it. The
//! desktop is snapshotted before the first preview; `commitPreview` keeps the
//! theme, `revertPreview` (or 15 seconds passing) restores the snapshot.

use std::{path::Path, rc::Rc, time::Duration};

use lumen_ipc::{
    ApplyPresetParams, GameModeChanged, ImportWallpaperParams, Notification, PREVIEW_SECONDS, PreviewState, PreviewThemeParams, RpcError,
    ThemeChangeReason, ThemeChanged, ThemeState, code, event, method,
};
use lumen_theme::{Mode, Source, SourceColors, Theme, Variant};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::{
    sync::{Notify, broadcast},
    time::Instant,
};
use tracing::{info, warn};

use crate::{
    desktop::{ApplyPlan, Desktop},
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
        Self { store, desktop, committed, preview: None, game: None, events: broadcast::channel(64).0, deadline_changed: Rc::new(Notify::new()) }
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
            method::APPLY_PRESET => json(self.apply_preset(parse(params)?)?),
            other => Err(RpcError::new(code::METHOD_NOT_FOUND, format!("unknown method \"{other}\""))),
        }
    }

    fn theme_state(&self) -> ThemeState {
        ThemeState {
            theme: self.committed.clone(),
            preview: self.preview.as_ref().map(|p| PreviewState {
                theme: p.theme.clone(),
                seconds_left: p.deadline.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64,
            }),
        }
    }

    fn preview_theme(&mut self, params: PreviewThemeParams) -> Result<PreviewState, RpcError> {
        let theme = self.build_theme(params)?;
        let plan = self.plan(&theme, None);

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
        self.emit_theme(None, ThemeChangeReason::Commit);
        Ok(preview.theme)
    }

    /// Puts the desktop back as it was before the preview. Not an error if
    /// nothing is being previewed.
    fn revert_preview(&mut self, reason: ThemeChangeReason) -> Result<(), RpcError> {
        let Some(preview) = self.preview.take() else { return Ok(()) };
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

    fn apply_preset(&mut self, params: ApplyPresetParams) -> Result<Theme, RpcError> {
        let preset = self.store.preset(&params.id).map_err(store_error)?;
        let theme = self.store.theme_file(&preset.theme).map_err(store_error)?;
        let wallpapers = preset
            .wallpapers
            .iter()
            .map(|(monitor, image)| ((monitor != "all").then(|| monitor.clone()), self.store.resolve(image)))
            .collect::<Vec<_>>();

        // A preset replaces whatever is being tried out.
        self.revert_preview(ThemeChangeReason::Revert)?;
        let plan = self.plan(&theme, Some(wallpapers).filter(|w| !w.is_empty()));
        self.desktop.apply(&plan).map_err(desktop_error)?;
        self.store.save_active_theme(&theme).map_err(store_error)?;
        info!("applied preset \"{}\"", preset.name);
        self.committed = Some(theme.clone());
        self.emit_theme(None, ThemeChangeReason::Preset);
        Ok(theme)
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
        self.emit(event::THEME_CHANGED, ThemeChanged { theme, preview: previewed.is_some(), reason });
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

    use lumen_ipc::WallpaperEntry;
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
    async fn apply_preset_sets_per_monitor_wallpapers_and_keeps_the_theme() {
        let mut service = service();
        let root = service.store.root().to_owned();
        let image = test_image(&root, "sky.png", [40, 80, 200]);
        let entry: WallpaperEntry = call(&mut service, method::IMPORT_WALLPAPER, json!({ "path": image }));
        let mut theme = Theme::from_color(Color::new(40, 80, 200), Variant::Tonal, Mode::Dark, 0).unwrap();
        theme.name = "Sky".into();
        fs::write(root.join("themes/sky.json"), serde_json::to_string(&theme).unwrap()).unwrap();
        fs::write(
            root.join("presets/sky.json"),
            json!({ "name": "Sky", "theme": "themes/sky.json", "wallpapers": { "MONITOR-A": entry.file, "MONITOR-B": entry.file } }).to_string(),
        )
        .unwrap();

        // A preview in progress is replaced, not left dangling.
        preview_blue(&mut service);
        let mut rx = service.subscribe();
        let applied: Theme = call(&mut service, method::APPLY_PRESET, json!({ "id": "sky" }));

        assert_eq!(applied, theme);
        assert!(service.preview.is_none());
        assert_eq!(service.store.active_theme(), Some(theme));
        let plan = service.desktop.current.clone().unwrap();
        let image = PathBuf::from(&entry.path);
        assert_eq!(plan.wallpapers, vec![(Some("MONITOR-A".into()), image.clone()), (Some("MONITOR-B".into()), image)]);
        let reasons: Vec<_> = events(&mut rx).iter().map(|e| e["params"]["reason"].clone()).collect();
        assert_eq!(reasons, [json!("revert"), json!("preset")]);
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
