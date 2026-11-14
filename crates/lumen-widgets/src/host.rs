//! Runs the widgets: owns their windows, timers, data and layout, and
//! redraws a widget only when what it shows has changed.
//!
//! The host lives on one thread (the one that calls [`start`]) and is driven
//! by that thread's message loop. Other threads reach it through a
//! [`HostHandle`], which queues a [`Command`] and wakes it. Whenever the
//! layout, the selection or edit mode changes, the host reports the new
//! [`WidgetsState`] through the callback given to [`start`].
//!
//! Desktop edit mode (Win+Alt+E) dims every monitor, raises the widgets above
//! other windows and makes them draggable; Esc or the hotkey again leaves it.

mod bars;
mod files;
mod flyouts;
mod ha;
mod launchers;
mod styles;
mod scripts;
mod weathers;

use std::{
    cell::RefCell,
    collections::HashMap,
    collections::VecDeque,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use lumen_ipc::bar::BarConfig;
use lumen_ipc::flyout::FlyoutConfig;
use lumen_ipc::launcher::LauncherConfig;
use lumen_ipc::style::StyleSheet;
use lumen_ipc::widgets::{
    AddWidgetParams, Guides, Layout, LayoutOptionsParams, MIN_SIZE, MonitorArea, Placement, UpdateWidgetParams, WidgetCost, WidgetKind as Kind, WidgetsState,
};
use lumen_render::{Bitmap, Error as RenderError, Gpu, Surface, SurfaceOptions, set_input_handler};
use lumen_theme::Theme;
use tracing::{info, warn};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromRect},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::{GetKeyState, MOD_ALT, MOD_NOREPEAT, MOD_WIN, RegisterHotKey, ReleaseCapture, SetCapture, UnregisterHotKey, VK_ESCAPE, VK_SHIFT},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, IDC_ARROW, IDC_HAND, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENWSE, KillTimer, LoadCursorW, PostMessageW,
                RegisterClassW, SetCursor, SetTimer, WINDOW_STYLE, WM_APP, WM_CAPTURECHANGED, WM_DISPLAYCHANGE, WM_ACTIVATE, WM_CHAR, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_SETCURSOR, WM_TIMECHANGE, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::w,
};

use crate::{
    BUNDLED_FONTS, bar,
    clock::{self, ClockData},
    edit,
    governor::{self, Governor},
    layout,
    media::{self, MediaInfo},
    paint::Paint,
    parts,
    snap::{self, Frame},
    stats::{self, StatsData, StatsSampler},
};

const WAKE: u32 = WM_APP + 10;
const TIMER_CLOCK: usize = 1;
const TIMER_STATS: usize = 2;
const TIMER_MEDIA: usize = 3;
const TIMER_HOVER: usize = 4;
/// How often the now-playing progress bar moves while something is playing.
const MEDIA_TICK_MS: u32 = 1000;
/// How often the mouse position is checked, only while a "show on hover" widget exists.
const HOVER_TICK_MS: u32 = 120;
const HOTKEY_EDIT: i32 = 1;
const HOTKEY_ESCAPE: i32 = 2;
/// While dragging, the settings app is told at most this often.
const DRAG_REPORT_EVERY: Duration = Duration::from_millis(40);
/// Windows posts this to a bar's anchor window when reserved screen space changes.
const BAR_CALLBACK: u32 = WM_APP + 11;

/// How to run the widgets.
#[derive(Debug, Clone)]
pub struct Options {
    /// Where the layout is kept (`widgets/layout.json`).
    pub layout_path: PathBuf,
    /// Where the bar's settings are kept (`bar.json`).
    pub bar_path: PathBuf,
    /// Where the flyout settings are kept (`flyouts.json`).
    pub flyouts_path: PathBuf,
    /// Where the launcher's settings are kept (`launcher.json`).
    pub launcher_path: PathBuf,
    /// Where the user's component styles are kept (`styles.json`).
    pub styles_path: PathBuf,
    /// Font files the user added to Lumen (`fonts/`); loaded beside the bundled ones.
    pub fonts_dir: PathBuf,
    /// The folder of widget files (`widgets/`): every `.yaml` in it is read, and read again when saved.
    pub widget_files_dir: PathBuf,
    /// Where the weather settings are kept (`weather.json`); the last forecast is kept beside it.
    pub weather_path: PathBuf,
    /// Where the Home Assistant connection is kept (`home-assistant.json`), token included; it never leaves this PC.
    pub home_assistant_path: PathBuf,
    /// Where the scripts the user has agreed to are noted (`script-approvals.json`).
    pub script_approvals_path: PathBuf,
    /// Keep widgets above other windows (for testing; normally they sit on the desktop).
    pub topmost: bool,
}

/// Something for the host to do, sent from another thread.
#[derive(Debug, Clone)]
pub enum Command {
    /// Recolour every widget.
    Theme(Box<Theme>),
    Update(UpdateWidgetParams),
    Add(AddWidgetParams),
    Remove(String),
    Select(Option<String>),
    Options(LayoutOptionsParams),
    EditMode(bool),
    /// Replace the whole layout (applying a preset).
    SetLayout(Box<Layout>),
    /// Let a widget the governor switched off run again.
    Resume(String),
    /// Replace the bar's settings.
    SetBar(Box<BarConfig>),
    /// Replace the flyout settings.
    SetFlyouts(Box<FlyoutConfig>),
    /// Show the volume flyout once, as it looks now.
    ShowFlyout,
    /// Replace the launcher's settings.
    SetLauncher(LauncherConfig),
    /// Replace the user's component styles and what uses which.
    SetStyles(Box<StyleSheet>),
    /// A font file was added: load the fonts again and redraw.
    ReloadFonts,
    /// Replace the weather settings.
    SetWeather(lumen_ipc::weather::WeatherConfig),
    /// Fetch the forecast now.
    RefreshWeather,
    /// Replace the Home Assistant connection, and try it.
    SetHomeAssistant(lumen_ipc::home_assistant::HaConfig),
    /// Let a widget file's script run: the user agreed to the text with this fingerprint.
    ApproveScript { file: String, hash: String },
    /// Stop a widget file's script and forget that it was allowed.
    RevokeScript(String),
}

/// What the host tells its owner.
#[derive(Debug, Clone)]
pub enum Report {
    /// The bar's settings changed (also sent once at start-up).
    Bar(BarConfig),
    /// The flyout settings changed (also sent once at start-up).
    Flyouts(FlyoutConfig),
    /// The launcher's settings changed (also sent once at start-up).
    Launcher(LauncherConfig),
    /// The component styles changed (also sent once at start-up).
    Styles(StyleSheet),
    /// The widget files changed: one was added, saved, or has a mistake (also sent once at start-up).
    Files(Vec<lumen_ipc::widget_file::WidgetFileInfo>),
    /// The weather settings or the forecast changed (also sent once at start-up).
    Weather(lumen_ipc::weather::WeatherState),
    /// The Home Assistant connection changed or was tried (also sent once at start-up). Never holds the token.
    HomeAssistant(lumen_ipc::home_assistant::HaState),
    /// The scripts, or whether one may run, changed (also sent once at start-up).
    Scripts(Vec<lumen_ipc::scripts::ScriptInfo>),
    /// The layout, selection, edit mode or list of switched-off widgets changed.
    State(WidgetsState),
    /// Fresh per-widget costs, about every ten seconds while widgets are running.
    Costs(Vec<WidgetCost>),
}

/// Lets other threads talk to the host.
#[derive(Clone)]
pub struct HostHandle {
    window: isize,
    inbox: Arc<Mutex<Inbox>>,
}

#[derive(Default)]
struct Inbox {
    commands: VecDeque<Command>,
    /// `Some(None)` means "nothing is playing now".
    media: Option<Option<MediaInfo>>,
    /// A forecast fetch that finished, with the settings it was for.
    weather: Option<(lumen_ipc::weather::WeatherConfig, Result<lumen_ipc::weather::WeatherData, String>)>,
    /// Script calls that finished.
    scripts: Vec<scripts::ScriptResult>,
    /// Home Assistant calls that finished.
    ha: Vec<ha::HaResult>,
}

impl HostHandle {
    pub fn send(&self, command: Command) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).commands.push_back(command);
        self.wake();
    }

    fn set_media(&self, media: Option<MediaInfo>) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).media = Some(media);
        self.wake();
    }

    fn set_weather(&self, config: lumen_ipc::weather::WeatherConfig, result: Result<lumen_ipc::weather::WeatherData, String>) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).weather = Some((config, result));
        self.wake();
    }

    fn ha_result(&self, result: ha::HaResult) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).ha.push(result);
        self.wake();
    }

    fn script_result(&self, result: scripts::ScriptResult) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).scripts.push(result);
        self.wake();
    }

    fn wake(&self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.window as *mut _)), WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

/// A monitor, as the host needs it.
struct Area {
    info: MonitorArea,
    /// Work area origin and the whole monitor's rectangle, in physical pixels.
    work: (i32, i32),
    /// The whole work area: x, y, width, height.
    work_rect: (i32, i32, i32, i32),
    bounds: (i32, i32, i32, i32),
    scale: f32,
}

struct Slot {
    id: String,
    kind: Kind,
    surface: Surface,
    /// Index into `Host::areas`.
    area: usize,
    /// "Show on hover" widgets: whether the mouse is over it now.
    hovered: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move,
    /// Resizing by a corner: `(right side, bottom side)` of the widget.
    Resize(bool, bool),
}

struct Drag {
    id: String,
    mode: DragMode,
    /// Where in the widget it was grabbed, in physical pixels from its top-left.
    grab: (i32, i32),
    /// The corner that stays put while resizing, in logical pixels.
    anchor: (f32, f32),
    /// The widget's size and contents scale when the drag began (for scaling the contents with Shift held).
    start: (f32, f32, f32),
    last_report: Instant,
}

/// Everything that exists only while edit mode is on.
struct Edit {
    dims: Vec<HWND>,
    selection: Option<Surface>,
    guide_x: Option<Surface>,
    guide_y: Option<Surface>,
    hint: Option<Surface>,
    drag: Option<Drag>,
}

struct Host {
    window: HWND,
    options: Options,
    layout: Layout,
    paint: Paint,
    /// A fullscreen app or game is in front.
    paused: bool,
    /// The user chose "Pause widgets" in the tray menu: everything is hidden.
    user_paused: bool,
    inbox: Arc<Mutex<Inbox>>,
    on_state: Box<dyn Fn(Report)>,
    governor: Governor,
    costs_sent_at: Instant,

    /// `None` while rendering is suspended (a game is running, or the user
    /// paused the widgets): no graphics device and no widget windows exist.
    gpu: Option<Rc<Gpu>>,
    areas: Vec<Area>,
    slots: Vec<Slot>,
    selected: Option<String>,
    guides: Guides,
    edit: Option<Edit>,

    clock: ClockData,
    sampler: StatsSampler,
    stats: StatsData,
    media: Option<MediaInfo>,
    /// The current track's cover, uploaded to the GPU on first use.
    art: Option<Bitmap>,

    /// The status bar's settings, and one entry per monitor it is on.
    bar: BarConfig,
    bars: Vec<bars::BarSlot>,
    /// Launcher icons as read from disk (`None`: the file has no icon), and on the GPU.
    icon_pixels: HashMap<String, Option<lumen_win::shell::Icon>>,
    icons: HashMap<String, Bitmap>,

    flyouts: FlyoutConfig,
    /// The default playback device's volume, held while Lumen answers the volume keys.
    volume: Option<lumen_win::audio::Volume>,
    /// The volume flyout, while it is on screen.
    flyout: Option<flyouts::Shown>,

    launcher: launchers::Launcher,

    /// The user's component styles, and the paint each kind of piece draws with.
    styles: StyleSheet,
    paints: styles::Paints,

    files: files::Files,

    weather: weathers::Weather,
    scripts: scripts::Scripts,
    ha: ha::Ha,

    /// How the system readings have moved lately, for graph parts.
    history: parts::History,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Starts the widgets on this thread, which must run a message loop.
/// `on_report` is called on this thread with every change to the layout,
/// selection or edit mode (starting with the state at start-up), and with
/// per-widget costs as they are measured.
pub fn start(theme: &Theme, options: Options, on_report: impl Fn(Report) + 'static) -> Result<HostHandle, String> {
    let window = unsafe {
        let instance = GetModuleHandleW(None).map_err(text)?;
        let class = w!("LumenWidgetHost");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
        // Hidden, but top-level so it hears about display changes.
        CreateWindowExW(WS_EX_TOOLWINDOW, class, w!("Lumen widgets"), WINDOW_STYLE(0), 0, 0, 0, 0, None, None, Some(instance.into()), None).map_err(text)?
    };

    let inbox = Arc::new(Mutex::new(Inbox::default()));
    let mut sampler = StatsSampler::new();
    let mut host = Host {
        window,
        layout: layout::load_or_create(&options.layout_path),
        options,
        paint: Paint::from_theme(theme),
        paused: false,
        user_paused: false,
        inbox: inbox.clone(),
        on_state: Box::new(on_report),
        governor: Governor::new(Instant::now()),
        costs_sent_at: Instant::now(),
        gpu: None,
        areas: Vec::new(),
        slots: Vec::new(),
        selected: None,
        guides: Guides::default(),
        edit: None,
        clock: ClockData::now(),
        stats: sampler.sample(),
        sampler,
        media: None,
        art: None,
        bar: BarConfig::default(),
        bars: Vec::new(),
        icon_pixels: HashMap::new(),
        icons: HashMap::new(),
        flyouts: FlyoutConfig::default(),
        volume: None,
        flyout: None,
        launcher: launchers::Launcher::default(),
        styles: StyleSheet::default(),
        paints: styles::Paints::plain(&Paint::from_theme(theme)),
        files: files::Files::default(),
        weather: weathers::Weather::default(),
        scripts: scripts::Scripts::default(),
        ha: ha::Ha::default(),
        history: parts::History::default(),
    };
    host.start_files();
    host.load_weather();
    host.load_scripts();
    host.load_ha();
    host.styles = styles::load_styles(&host.options.styles_path);
    host.refresh_paints();
    host.bar = bars::load_bar(&host.options.bar_path);
    host.flyouts = flyouts::load_flyouts(&host.options.flyouts_path);
    host.load_launcher();
    host.rebuild()?;
    host.load_icons();
    host.place_bars();
    host.draw(None);
    host.sync_timers();
    host.sync_volume_keys();
    host.sync_launcher_hotkey();
    unsafe {
        if RegisterHotKey(Some(window), HOTKEY_EDIT, MOD_WIN | MOD_ALT | MOD_NOREPEAT, u32::from(b'E')).is_err() {
            warn!("Win+Alt+E is taken by another app; edit mode can still be started from Lumen");
        }
    }
    set_input_handler(Some(on_input));
    info!("{} widgets on the desktop{}", host.slots.len(), if host.bar.enabled { ", bar on" } else { "" });
    host.report();
    (host.on_state)(Report::Bar(host.bar.clone()));
    (host.on_state)(Report::Flyouts(host.flyouts.clone()));
    (host.on_state)(Report::Launcher(host.launcher.config.clone()));
    (host.on_state)(Report::Styles(host.styles.clone()));
    (host.on_state)(Report::Files(host.files.list.clone()));
    (host.on_state)(Report::Weather(host.weather.state.clone()));
    HOST.set(Some(host));

    let handle = HostHandle { window: window.0 as isize, inbox };
    let for_media = handle.clone();
    media::watch(move |info| for_media.set_media(info));
    Ok(handle)
}

/// Tells the host whether a fullscreen app or game is in front. Widgets set
/// to hide in games stop drawing and disappear until it is gone.
pub fn set_paused(paused: bool) {
    with_host(|host| {
        if host.paused == paused {
            return;
        }
        host.paused = paused;
        if paused {
            // Editing the desktop makes no sense under a game.
            host.set_edit_mode(false);
        } else {
            host.clock = ClockData::now();
            host.stats = host.sampler.sample();
        }
        host.sync_volume_keys();
        host.sync_launcher_hotkey();
        host.sync_suspension();
        host.sync_visibility();
        host.draw(None);
        host.sync_timers();
    });
}

/// Hides every widget and stops all their work until switched back
/// ("Pause widgets" in the tray menu). Call on the host's thread.
pub fn set_user_paused(paused: bool) {
    with_host(|host| {
        if host.user_paused == paused {
            return;
        }
        host.user_paused = paused;
        if paused {
            host.set_edit_mode(false);
        } else {
            host.clock = ClockData::now();
            host.stats = host.sampler.sample();
        }
        host.sync_suspension();
        host.sync_visibility();
        host.draw(None);
        host.sync_timers();
        host.report();
    });
}

/// Whether the user has paused the widgets. Call on the host's thread.
pub fn is_user_paused() -> bool {
    with_host(|host| host.user_paused).unwrap_or(false)
}

/// Whether desktop edit mode is on. Call on the host's thread.
pub fn is_editing() -> bool {
    with_host(|host| host.edit.is_some()).unwrap_or(false)
}

/// Turns desktop edit mode on or off, as Win+Alt+E does. Call on the host's thread.
pub fn set_edit_mode(on: bool) {
    with_host(|host| host.run(Command::EditMode(on)));
}

/// Removes the widgets. Call on the host's thread before it exits.
pub fn stop() {
    with_host(|host| {
        host.set_edit_mode(false);
        host.hide_flyout();
        // Hand the volume keys back to Windows.
        host.flyouts.volume = false;
        host.sync_volume_keys();
        host.launcher.config.enabled = false;
        host.sync_launcher_hotkey();
    });
    set_input_handler(None);
    if let Some(mut host) = HOST.take() {
        host.stop_scripts();
        unsafe {
            for timer in [TIMER_CLOCK, TIMER_STATS, TIMER_MEDIA, TIMER_HOVER, files::TIMER_FILES, weathers::TIMER_WEATHER, scripts::TIMER_SCRIPTS, ha::TIMER_HA] {
                let _ = KillTimer(Some(host.window), timer);
            }
            let _ = UnregisterHotKey(Some(host.window), HOTKEY_EDIT);
            let _ = DestroyWindow(host.window);
        }
    }
}

fn with_host<T>(action: impl FnOnce(&mut Host) -> T) -> Option<T> {
    HOST.with(|cell| {
        // If the host is already busy (a nested message), skip rather than panic.
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(action)
    })
}

impl Host {
    // ----- set-up -----

    /// (Re)creates the graphics device and every widget window, then draws them.
    fn rebuild(&mut self) -> Result<(), String> {
        let editing = self.edit.is_some();
        self.set_edit_mode(false);
        self.slots.clear();
        self.art = None;
        self.gpu = None;
        self.areas = read_areas();
        self.drop_bar_surfaces();
        self.hide_flyout();
        self.close_launcher();
        if self.should_suspend() {
            // A display change during a game: note the new monitors, but stay
            // suspended. Everything is rebuilt when the game ends.
            return Ok(());
        }
        // The bundled fonts, then any font files the user added.
        let added = imported_fonts(&self.options.fonts_dir);
        let fonts: Vec<&[u8]> = BUNDLED_FONTS.iter().copied().chain(added.iter().map(Vec::as_slice)).collect();
        self.gpu = Some(Gpu::new(&fonts).map_err(text)?);
        self.make_bar_surfaces();

        for index in 0..self.layout.widgets.len() {
            let placement = self.layout.widgets[index].clone();
            if let Some(slot) = self.create_slot(&placement) {
                self.slots.push(slot);
            }
        }
        self.sync_visibility();
        self.draw(None);
        if editing {
            self.set_edit_mode(true);
        }
        Ok(())
    }

    fn create_slot(&mut self, placement: &Placement) -> Option<Slot> {
        let gpu = self.gpu.clone()?;
        let area = self.area_index(placement.monitor.as_deref());
        let (x, y) = self.to_physical(area, placement.x, placement.y);
        let editing = self.edit.is_some();
        let options = SurfaceOptions { x, y, width: placement.width, height: placement.height, click_through: !editing, topmost: editing || self.options.topmost };
        match Surface::new(&gpu, options) {
            Ok(surface) => {
                surface.set_opacity(f32::from(placement.style.opacity) / 100.0);
                Some(Slot { id: placement.id.clone(), kind: placement.kind, surface, area, hovered: false })
            }
            Err(error) => {
                warn!("could not create the widget {}: {error}", placement.id);
                None
            }
        }
    }

    // ----- geometry -----

    /// The monitor a placement is on: the one named, else the primary, else the first.
    fn area_index(&self, monitor: Option<&str>) -> usize {
        monitor.and_then(|id| self.areas.iter().position(|a| a.info.id == id)).or_else(|| self.areas.iter().position(|a| a.info.primary)).unwrap_or(0)
    }

    fn to_physical(&self, area: usize, x: f32, y: f32) -> (i32, i32) {
        match self.areas.get(area) {
            Some(area) => (area.work.0 + (x * area.scale).round() as i32, area.work.1 + (y * area.scale).round() as i32),
            None => (x.round() as i32, y.round() as i32),
        }
    }

    fn area_size(&self, area: usize) -> (f32, f32) {
        self.areas.get(area).map_or((1920.0, 1080.0), |a| (a.info.width, a.info.height))
    }

    /// The other widgets on a monitor, for snapping against.
    fn frames_on(&self, area: usize, except: &str) -> Vec<Frame> {
        self.layout
            .widgets
            .iter()
            .filter(|w| w.id != except && self.area_index(w.monitor.as_deref()) == area)
            .map(|w| Frame { x: w.x, y: w.y, width: w.width, height: w.height })
            .collect()
    }

    fn placement(&self, id: &str) -> Option<&Placement> {
        self.layout.widgets.iter().find(|w| w.id == id)
    }

    // ----- reporting and saving -----

    fn state(&self) -> WidgetsState {
        WidgetsState {
            layout: self.layout.clone(),
            monitors: self.areas.iter().map(|a| a.info.clone()).collect(),
            edit_mode: self.edit.is_some(),
            selected: self.selected.clone(),
            guides: self.guides.clone(),
            suspended: self.governor.suspended(),
            // So the app can draw a widget that answered its file's options as the desktop does.
            resolved: self.layout.widgets.iter().filter(|w| w.file.is_some() && !w.options.is_empty()).map(|w| (w.id.clone(), self.parts_of(w).0)).collect(),
        }
    }

    fn report(&self) {
        (self.on_state)(Report::State(self.state()));
    }

    /// What each widget costs right now: measured CPU, and the memory it holds.
    fn costs(&self) -> Vec<WidgetCost> {
        self.layout
            .widgets
            .iter()
            .map(|placement| {
                let slot = self.slots.iter().find(|slot| slot.id == placement.id);
                let mut bytes = slot.map_or(0, |slot| slot.surface.memory_bytes());
                if placement.kind == Kind::NowPlaying
                    && let Some(art) = self.media.as_ref().and_then(|m| m.art.as_ref())
                {
                    // The decoded cover, plus its copy on the GPU.
                    bytes += art.rgba.len() as u64 * 2;
                }
                self.governor.cost(&placement.id, bytes as f64 / (1024.0 * 1024.0))
            })
            // The bar is measured like a widget, under its own name.
            .chain(self.bar.enabled.then(|| self.governor.cost(bars::BAR_COST_ID, self.bar_memory_bytes() as f64 / (1024.0 * 1024.0))))
            .collect()
    }

    /// Closes the governor's measuring window when it is due: switches off
    /// any widget that has been far over budget, and reports the new costs.
    fn govern(&mut self) {
        let now = Instant::now();
        let switched_off = self.governor.roll(now);
        if !switched_off.is_empty() {
            for id in &switched_off {
                warn!("the widget {id} was switched off for using too much CPU");
            }
            self.sync_visibility();
            self.sync_timers();
            self.report();
        }
        if now.duration_since(self.costs_sent_at) >= governor::WINDOW {
            self.costs_sent_at = now;
            (self.on_state)(Report::Costs(self.costs()));
        }
    }

    // ----- suspending everything -----

    /// Whether nothing at all should be on screen: the user paused the
    /// widgets, or a game is in front and every widget hides in games.
    fn should_suspend(&self) -> bool {
        // "Pause widgets" leaves the bar, the flyout and the launcher working, so the device is still needed then.
        (self.user_paused && !self.bar.enabled && !self.flyouts.volume && !self.launcher.config.enabled) || (self.paused && self.layout.widgets.iter().all(|w| w.style.hide_in_games))
    }

    /// Suspends or resumes rendering as a whole. Suspended means the widget
    /// windows and the graphics device are gone, not merely hidden: no GPU
    /// memory, no timers, and the media listener ignores what Windows tells it.
    fn sync_suspension(&mut self) {
        let suspend = self.should_suspend();
        if suspend && self.gpu.is_some() {
            self.set_edit_mode(false);
            self.slots.clear();
            self.drop_bar_surfaces();
            self.hide_flyout();
            self.close_launcher();
            self.art = None;
            self.gpu = None;
            media::set_paused(true);
            info!("rendering suspended");
        } else if !suspend && self.gpu.is_none() {
            media::set_paused(false);
            if let Err(error) = self.rebuild() {
                warn!("could not bring the widgets back: {error}");
            }
            info!("rendering resumed");
        }
    }

    fn save(&self) {
        if let Err(error) = layout::save(&self.options.layout_path, &self.layout) {
            warn!("{error}");
        }
    }

    // ----- visibility, drawing, timers -----

    /// Whether a widget should be on screen right now.
    fn wants_visible(&self, slot: &Slot) -> bool {
        let Some(placement) = self.placement(&slot.id) else {
            return false;
        };
        if self.user_paused || (self.paused && placement.style.hide_in_games) || self.governor.is_suspended(&slot.id) {
            return false;
        }
        // In edit mode everything shows, so it can be grabbed.
        self.edit.is_some() || !placement.style.show_on_hover || slot.hovered
    }

    fn sync_visibility(&mut self) {
        for index in 0..self.slots.len() {
            let visible = self.wants_visible(&self.slots[index]);
            self.slots[index].surface.set_visible(visible);
        }
    }

    /// Redraws the visible widgets of one kind, or all of them.
    fn draw(&mut self, only: Option<Kind>) {
        if only.is_none() {
            self.sync_interactive();
        }
        let mut lost = false;
        for index in 0..self.slots.len() {
            let (kind, id) = (self.slots[index].kind, self.slots[index].id.clone());
            if only.is_some_and(|wanted| !self.shows(&id, kind, wanted)) || !self.slots[index].surface.is_visible() {
                continue;
            }
            let Some(paint) = self.placement(&id).map(|placement| self.widget_paint(placement)) else {
                continue;
            };
            let (width, height) = self.slots[index].surface.size();
            // Its file's parts, its own, or the arrangement its kind comes with.
            let Some((parts, default_size)) = self.placement(&id).map(|placement| self.parts_of(placement)) else { continue };
            let zoom = self.placement(&id).map_or(1.0, |placement| placement.scale);
            let file = self.placement(&id).and_then(|placement| placement.file.clone());
            let (script, notice) = self.scripts.view(file.as_deref());
            let home = self.ha.view();
            let (clock, stats, media, weather, history) = (&self.clock, &self.stats, self.media.as_ref(), &self.weather.state, &self.history);
            let hovered = self.slots[index].hovered;
            let art = &mut self.art;
            let started = Instant::now();
            let result = self.slots[index].surface.draw(|canvas| {
                if art.is_none()
                    && let Some(cover) = media.and_then(|m| m.art.as_ref())
                {
                    *art = canvas.bitmap_from_rgba(cover.size, cover.size, &cover.rgba).ok();
                }
                let data = parts::Data { clock, stats, media, art: art.as_ref(), weather, history, hovered, script, ha: home, notice: notice.as_deref() };
                parts::draw(canvas, &paint, &parts, &data, (width, height), default_size, zoom);
            });
            self.governor.record_draw(&id, started.elapsed());
            match result {
                Ok(()) => {}
                Err(RenderError::DeviceLost) => lost = true,
                Err(error) => warn!("{error}"),
            }
        }
        if lost {
            info!("graphics device lost; recreating the widgets");
            if let Err(error) = self.rebuild() {
                warn!("could not recreate the widgets: {error}");
            }
            return;
        }
        // The bar shows the same data; redraw it when what changed is on it.
        let (clock, stats, media) = bar::needs(&self.bar);
        let affected = match only {
            None => true,
            Some(Kind::Clock) => clock,
            Some(Kind::SystemStats) => stats,
            Some(Kind::NowPlaying) => media,
            Some(Kind::Weather) => bar::needs_weather(&self.bar),
        };
        if affected {
            self.draw_bars();
        }
    }

    /// Lets a widget take the mouse while the desktop is being edited, or if
    /// one of its parts does something when clicked; otherwise clicks fall
    /// through it to whatever is underneath.
    fn sync_interactive(&mut self) {
        let editing = self.edit.is_some();
        for index in 0..self.slots.len() {
            let clickable = self.placement(&self.slots[index].id).is_some_and(|placement| parts::any_clickable(&self.parts_of(placement).0));
            self.slots[index].surface.set_interactive(editing || clickable);
        }
    }

    fn any_graph_visible(&self) -> bool {
        self.slots.iter().filter(|slot| slot.surface.is_visible()).filter_map(|slot| self.placement(&slot.id)).any(|placement| self.parts_of(placement).0.iter().any(|part| part.kind == lumen_ipc::parts::PartKind::Graph && !part.hidden))
    }

    /// The live values, for working out what a part shows or does outside drawing.
    fn part_data(&self, hovered: bool) -> parts::Data<'_> {
        parts::Data { clock: &self.clock, stats: &self.stats, media: self.media.as_ref(), art: None, weather: &self.weather.state, history: &self.history, hovered, script: None, ha: self.ha.view(), notice: None }
    }

    /// What a click at a point on a widget's window does, if anything (not while editing).
    fn widget_action(&self, window: isize, x: i32, y: i32) -> Option<lumen_ipc::behaviour::Action> {
        if self.edit.is_some() {
            return None;
        }
        let slot = self.slots.iter().find(|slot| slot.surface.window() == window)?;
        let placement = self.placement(&slot.id)?;
        let (parts, default_size) = self.parts_of(placement);
        let scale = slot.surface.scale();
        let data = parts::Data { script: self.scripts.view(placement.file.as_deref()).0, ..self.part_data(true) };
        parts::action_at(&parts, &data, slot.surface.size(), default_size, placement.scale, (x as f32 / scale, y as f32 / scale))
    }

    /// A widget was clicked (outside edit mode): do what the part under the mouse says.
    fn on_widget_click(&mut self, window: isize, x: i32, y: i32) {
        use lumen_ipc::behaviour::{Action, is_web_address};
        match self.widget_action(window, x, y) {
            Some(Action::ToggleMedia) => media::toggle_play_pause(),
            Some(Action::NextTrack) => media::skip(true),
            Some(Action::PreviousTrack) => media::skip(false),
            Some(Action::OpenLumen) => open_settings_app(),
            Some(Action::Open(target)) => {
                // A web page, or something that exists on this PC. Never a command line.
                let exists = std::path::Path::new(&target).exists();
                if !(is_web_address(&target) || exists) || !lumen_win::shell::open(&target) {
                    warn!("a widget tried to open {target}, which is not a web page or a file on this PC");
                }
            }
            Some(Action::HomeAssistant(entity)) => self.ha_clicked(entity),
            Some(Action::Script(part)) => {
                let file = self.slots.iter().find(|slot| slot.surface.window() == window).and_then(|slot| self.placement(&slot.id)).and_then(|placement| placement.file.clone());
                if let Some(file) = file {
                    self.script_clicked(&file, part);
                }
            }
            None => {}
        }
    }

    /// Whether any widget on screen shows this family of live values (the
    /// family is named by the widget kind that mainly shows it).
    fn any_visible(&self, kind: Kind) -> bool {
        self.slots.iter().any(|slot| slot.surface.is_visible() && self.shows(&slot.id, slot.kind, kind))
    }

    /// Whether a widget shows a family of live values: through the parts it
    /// came with, or through parts the user added (a clock with a CPU line).
    fn shows(&self, id: &str, kind: Kind, family: Kind) -> bool {
        let name = match family {
            Kind::Clock => "clock",
            Kind::SystemStats => "stats",
            Kind::NowPlaying => "media",
            Kind::Weather => "weather",
        };
        match self.placement(id) {
            Some(placement) if placement.parts.is_some() || placement.file.is_some() => lumen_ipc::parts::parts_use(&self.parts_of(placement).0, name),
            _ => kind == family,
        }
    }

    /// Runs exactly the timers that something on screen needs.
    fn sync_timers(&self) {
        let timer = |id: usize, wanted: bool, millis: u32| unsafe {
            if wanted {
                SetTimer(Some(self.window), id, millis, None);
            } else {
                let _ = KillTimer(Some(self.window), id);
            }
        };
        // The bar's modules need the same clock tick and stats samples as the widgets.
        let (bar_clock, bar_stats, _) = if self.any_bar_showing() { bar::needs(&self.bar) } else { (false, false, false) };
        timer(TIMER_CLOCK, self.any_visible(Kind::Clock) || bar_clock, clock::millis_until_next_minute());
        timer(TIMER_STATS, self.any_visible(Kind::SystemStats) || bar_stats, stats::SAMPLE_INTERVAL_MS);
        timer(TIMER_MEDIA, self.any_visible(Kind::NowPlaying) && self.media.as_ref().is_some_and(MediaInfo::is_advancing), MEDIA_TICK_MS);
        // Watching the mouse is only needed for widgets (or parts) that show on hover.
        let hovering = self.edit.is_none() && self.gpu.is_some() && self.layout.widgets.iter().any(|w| (w.style.show_on_hover || parts::any_hover_only(&self.parts_of(w).0)) && !(self.paused && w.style.hide_in_games));
        timer(TIMER_HOVER, hovering, HOVER_TICK_MS);
        // Weather is fetched only while something shows it; the timer is aimed at when the forecast goes stale.
        let weather_due = self.weather_due_in();
        timer(weathers::TIMER_WEATHER, weather_due.is_some(), weather_due.unwrap_or(0));
        // Home Assistant is asked only about entities a widget on screen mentions.
        let ha_due = self.ha_due_in();
        timer(ha::TIMER_HA, ha_due.is_some(), ha_due.unwrap_or(0));
        // Scripts run only while a widget that uses one is on screen.
        let scripts_due = self.scripts_due_in();
        timer(scripts::TIMER_SCRIPTS, scripts_due.is_some(), scripts_due.unwrap_or(0));
    }

    fn on_timer(&mut self, timer: usize) {
        if self.on_flyout_timer(timer) || self.on_files_timer(timer) || self.on_weather_timer(timer) || self.on_scripts_timer(timer) || self.on_ha_timer(timer) {
            return;
        }
        match timer {
            TIMER_CLOCK => {
                let now = ClockData::now();
                if now != self.clock {
                    self.clock = now;
                    self.draw(Some(Kind::Clock));
                }
                // Re-aim at the next minute boundary each time, so it never drifts.
                unsafe { SetTimer(Some(self.window), TIMER_CLOCK, clock::millis_until_next_minute(), None) };
            }
            TIMER_STATS => {
                let started = Instant::now();
                let now = self.sampler.sample();
                // Sampling is work done for the stats widgets: share its cost among them.
                let stats_widgets: Vec<String> = self.slots.iter().filter(|slot| slot.kind == Kind::SystemStats).map(|slot| slot.id.clone()).collect();
                let share = started.elapsed() / stats_widgets.len().max(1) as u32;
                for id in &stats_widgets {
                    self.governor.record_work(id, share);
                }
                self.history.push(&now);
                // A graph moves along at every reading, even when the value has not changed.
                if now != self.stats || self.any_graph_visible() {
                    self.stats = now;
                    self.draw(Some(Kind::SystemStats));
                }
            }
            TIMER_MEDIA => self.draw(Some(Kind::NowPlaying)),
            TIMER_HOVER => self.check_hover(),
            _ => {}
        }
        self.govern();
    }

    /// Shows "only on hover" widgets while the mouse is over where they sit.
    fn check_hover(&mut self) {
        let mut cursor = POINT::default();
        if unsafe { GetCursorPos(&mut cursor) }.is_err() {
            return;
        }
        let mut changed = false;
        for index in 0..self.slots.len() {
            let slot = &self.slots[index];
            let (x, y) = slot.surface.position();
            let (width, height) = slot.surface.size();
            let scale = slot.surface.scale();
            let inside = cursor.x >= x && cursor.y >= y && (cursor.x as f32) < x as f32 + width * scale && (cursor.y as f32) < y as f32 + height * scale;
            if inside != slot.hovered {
                self.slots[index].hovered = inside;
                changed = true;
            }
        }
        if changed {
            self.sync_visibility();
            self.clock = ClockData::now();
            self.draw(None);
            self.sync_timers();
        }
    }

    // ----- commands -----

    /// Handles what other threads left in the inbox.
    fn on_wake(&mut self) {
        let (commands, media, weather, script_results, ha_results) = {
            let mut inbox = self.inbox.lock().unwrap_or_else(|p| p.into_inner());
            (std::mem::take(&mut inbox.commands), inbox.media.take(), inbox.weather.take(), std::mem::take(&mut inbox.scripts), std::mem::take(&mut inbox.ha))
        };
        for result in ha_results {
            self.on_ha_result(result);
        }
        for result in script_results {
            self.on_script_result(result);
        }
        if let Some((config, result)) = weather {
            self.on_weather_result(config, result);
        }
        if let Some(media) = media {
            let same_cover = match (&media, &self.media) {
                (Some(new), Some(old)) => new.title == old.title && new.artist == old.artist && new.art.is_some() == old.art.is_some(),
                _ => false,
            };
            if !same_cover {
                self.art = None;
            }
            self.media = media;
            self.sync_timers();
            self.draw(Some(Kind::NowPlaying));
        }
        for command in commands {
            self.run(command);
        }
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::Theme(theme) => {
                self.paint = Paint::from_theme(&theme);
                self.refresh_paints();
                self.draw(None);
                self.draw_flyout();
                self.draw_launcher();
                if let Some(edit) = &self.edit {
                    edit::set_dim_colour(&self.paint);
                    edit.dims.iter().for_each(|dim| edit::repaint_dim(*dim));
                }
                self.sync_edit_surfaces();
            }
            Command::Update(params) => self.update_widget(params),
            Command::Add(params) => self.add_widget(params),
            Command::Remove(id) => {
                let before = self.layout.widgets.len();
                self.layout.widgets.retain(|w| w.id != id);
                if self.layout.widgets.len() != before {
                    self.governor.forget(&id);
                    self.slots.retain(|slot| slot.id != id);
                    if self.selected.as_deref() == Some(id.as_str()) {
                        self.selected = None;
                    }
                    self.after_layout_change(true);
                }
            }
            Command::Select(id) => {
                let id = id.filter(|id| self.placement(id).is_some());
                if id != self.selected {
                    self.selected = id;
                    self.sync_edit_surfaces();
                    self.report();
                }
            }
            Command::Options(options) => {
                if let Some(snap) = options.snap {
                    self.layout.snap = snap;
                }
                if let Some(locked) = options.locked {
                    self.layout.locked = locked;
                }
                self.after_layout_change(true);
            }
            Command::EditMode(on) => {
                if on != self.edit.is_some() {
                    self.set_edit_mode(on);
                    self.report();
                }
            }
            Command::SetBar(config) => self.set_bar(*config),
            Command::SetFlyouts(config) => self.set_flyouts(*config),
            Command::ShowFlyout => self.show_flyout_now(),
            Command::SetLauncher(config) => self.set_launcher(config),
            Command::SetStyles(sheet) => self.set_styles(*sheet),
            Command::SetWeather(config) => self.set_weather(config),
            Command::RefreshWeather => self.refresh_weather(),
            Command::SetHomeAssistant(config) => self.set_ha(config),
            Command::ApproveScript { file, hash } => self.approve_script(&file, &hash),
            Command::RevokeScript(file) => self.revoke_script(&file),
            Command::ReloadFonts => {
                if let Err(error) = self.rebuild() {
                    warn!("could not reload the fonts: {error}");
                }
                self.sync_timers();
            }
            Command::Resume(id) => {
                if self.governor.resume(&id) {
                    info!("the widget {id} was resumed");
                    self.sync_visibility();
                    self.draw(None);
                    self.sync_timers();
                    self.report();
                }
            }
            Command::SetLayout(layout) => {
                self.layout = layout.sanitised();
                self.selected = None;
                self.slots.clear();
                for index in 0..self.layout.widgets.len() {
                    let placement = self.layout.widgets[index].clone();
                    if let Some(slot) = self.create_slot(&placement) {
                        self.slots.push(slot);
                    }
                }
                self.sync_visibility();
                self.draw(None);
                self.after_layout_change(true);
            }
        }
    }

    /// Common tail of every layout change: refresh what is on screen, save if
    /// the change is final, and tell the settings app.
    fn after_layout_change(&mut self, save: bool) {
        self.sync_visibility();
        self.sync_timers();
        self.sync_edit_surfaces();
        if save {
            self.guides = Guides::default();
            self.sync_edit_surfaces();
            self.save();
        }
        self.report();
    }

    fn update_widget(&mut self, params: UpdateWidgetParams) {
        let Some(index) = self.layout.widgets.iter().position(|w| w.id == params.id) else {
            return;
        };
        let before = self.layout.widgets[index].clone();
        let mut after = before.clone();
        if let Some(monitor) = params.monitor.filter(|id| self.areas.iter().any(|a| &a.info.id == id)) {
            after.monitor = Some(monitor);
        }
        if let Some(style) = params.style {
            after.style = style;
            after.style.opacity = after.style.opacity.clamp(20, 100);
        }
        if let Some(id) = params.component_style {
            // Empty, or a style that is not there: back to the one all widgets share.
            after.component_style = Some(id).filter(|id| !id.is_empty() && self.styles.exists(id));
        }
        if let Some(options) = params.options {
            // Plain values only, and not an endless number of them; the file decides which mean anything.
            after.options = options.into_iter().filter(|(name, value)| name.len() <= 64 && (value.is_number() || value.is_boolean() || value.as_str().is_some_and(|text| text.len() <= 400))).take(lumen_ipc::widget_file::MAX_OPTIONS).collect();
        }
        if let Some(file) = params.file {
            // Empty: back to parts of its own. Otherwise the file takes over from them.
            after.file = Some(file).filter(|file| lumen_ipc::widget_file::is_file_name(file));
            if after.file.is_some() {
                after.parts = None;
            }
        }
        if let Some(parts) = params.parts {
            match after.file.clone() {
                // A widget from a file: the change goes into the file (an empty list would wipe it, so is ignored).
                Some(file) if !parts.is_empty() => self.set_file_parts(&file, parts, params.dragging, &after.options.clone()),
                Some(_) => {}
                // An empty list: back to the arrangement its kind comes with.
                None => after.parts = Some(lumen_ipc::parts::sanitise_parts(parts)).filter(|parts| !parts.is_empty()),
            }
        }
        if let Some(scale) = params.scale.filter(|scale| scale.is_finite()) {
            after.scale = scale.clamp(lumen_ipc::widgets::SCALE_RANGE.0, lumen_ipc::widgets::SCALE_RANGE.1);
        }
        // One of the nine named places: worked out here for the size the widget will have, and
        // taken exactly, not nudged to the grid or a neighbour.
        if let Some(anchor) = params.anchor.as_deref()
            && !self.layout.locked
        {
            let area = self.area_size(self.area_index(after.monitor.as_deref()));
            let size = (params.width.unwrap_or(after.width).clamp(MIN_SIZE.0, 2000.0), params.height.unwrap_or(after.height).clamp(MIN_SIZE.1, 2000.0));
            if let Some((x, y)) = lumen_ipc::widgets::anchor_position(anchor, size, area) {
                (after.width, after.height, after.x, after.y) = (size.0, size.1, x, y);
                self.guides = Guides::default();
                self.layout.widgets[index] = after.clone();
                self.apply_placement(&after, before.monitor != after.monitor);
                self.after_layout_change(true);
                return;
            }
        }
        let moved = params.x.is_some() || params.y.is_some();
        let resized = params.width.is_some() || params.height.is_some();
        let relocating = (moved || resized || after.monitor != before.monitor) && !self.layout.locked;
        if relocating {
            let area = self.area_index(after.monitor.as_deref());
            if resized {
                // Typed sizes are taken as given; only dragging rounds to the grid.
                let (width, height) = snap::snap_size(params.width.unwrap_or(after.width), params.height.unwrap_or(after.height), self.layout.snap && params.dragging);
                (after.width, after.height) = (width.min(2000.0), height.min(2000.0));
            }
            let wanted = Frame { x: params.x.unwrap_or(after.x), y: params.y.unwrap_or(after.y), width: after.width, height: after.height };
            // Snap while dragging and on drop; a typed coordinate lands exactly where asked.
            let snapping = self.layout.snap && moved && (params.dragging || params.x.is_some() && params.y.is_some());
            let snapped = snap::snap_move(wanted, &self.frames_on(area, &after.id), self.area_size(area), snapping);
            (after.x, after.y) = (snapped.x, snapped.y);
            self.guides =
                if params.dragging { Guides { x: snapped.guide_x, y: snapped.guide_y, monitor: self.areas.get(area).map(|a| a.info.id.clone()) } } else { Guides::default() };
        }
        if after == before && !params.dragging && self.guides == Guides::default() {
            return;
        }
        self.layout.widgets[index] = after.clone();
        self.apply_placement(&after, before.monitor != after.monitor);
        self.after_layout_change(!params.dragging);
    }

    /// Makes a widget's window match its placement.
    fn apply_placement(&mut self, placement: &Placement, monitor_changed: bool) {
        let area = self.area_index(placement.monitor.as_deref());
        let Some(index) = self.slots.iter().position(|slot| slot.id == placement.id) else {
            return;
        };
        let scale_changed = self.areas.get(area).is_some_and(|a| (a.scale - self.slots[index].surface.scale()).abs() > 0.001);
        if monitor_changed && scale_changed {
            // A different monitor scale needs a new window at that scale.
            if let Some(slot) = self.create_slot(placement) {
                self.slots[index] = slot;
            }
        } else {
            let (x, y) = self.to_physical(area, placement.x, placement.y);
            let slot = &mut self.slots[index];
            slot.area = area;
            slot.surface.set_position(x, y);
            if let Err(error) = slot.surface.resize(placement.width, placement.height) {
                warn!("{error}");
            }
            slot.surface.set_opacity(f32::from(placement.style.opacity) / 100.0);
        }
        let kind = placement.kind;
        self.sync_visibility();
        self.draw(Some(kind));
    }

    fn add_widget(&mut self, params: AddWidgetParams) {
        let area = self.area_index(params.monitor.as_deref());
        let file = params.file.filter(|file| lumen_ipc::widget_file::is_file_name(file));
        let size = file.as_deref().and_then(|file| self.file_size(file)).unwrap_or_else(|| params.kind.default_size());
        let others = self.frames_on(area, "");
        let (x, y) = match (params.x, params.y) {
            (Some(x), Some(y)) => {
                let snapped = snap::snap_move(Frame { x, y, width: size.0, height: size.1 }, &others, self.area_size(area), self.layout.snap);
                (snapped.x, snapped.y)
            }
            _ => snap::free_spot(size, &others, self.area_size(area)),
        };
        let placement = Placement {
            id: self.layout.next_id(params.kind),
            kind: params.kind,
            monitor: self.areas.get(area).filter(|a| !a.info.primary).map(|a| a.info.id.clone()),
            x,
            y,
            width: size.0,
            height: size.1,
            style: Default::default(),
            component_style: None,
            parts: None,
            file,
            options: Default::default(),
            scale: 1.0,
        };
        if let Some(slot) = self.create_slot(&placement) {
            self.slots.push(slot);
        }
        self.selected = Some(placement.id.clone());
        let kind = placement.kind;
        self.layout.widgets.push(placement);
        self.sync_visibility();
        let _ = kind;
        self.draw(None);
        self.sync_timers();
        self.after_layout_change(true);
    }

    // ----- edit mode -----

    fn set_edit_mode(&mut self, on: bool) {
        if on == self.edit.is_some() || (on && (self.paused || self.user_paused)) {
            return;
        }
        if on {
            edit::set_dim_colour(&self.paint);
            let dims = self.areas.iter().filter_map(|area| edit::create_dim(area.bounds, &self.paint, on_dim_click)).collect();
            self.edit = Some(Edit { dims, selection: None, guide_x: None, guide_y: None, hint: None, drag: None });
            unsafe {
                let _ = RegisterHotKey(Some(self.window), HOTKEY_ESCAPE, MOD_NOREPEAT, u32::from(VK_ESCAPE.0));
            }
            info!("edit mode on");
        } else {
            if let Some(edit) = self.edit.take() {
                edit.dims.into_iter().for_each(edit::destroy_dim);
            }
            unsafe {
                let _ = UnregisterHotKey(Some(self.window), HOTKEY_ESCAPE);
                let _ = ReleaseCapture();
            }
            self.guides = Guides::default();
            self.save();
            info!("edit mode off");
        }
        // Raise the widgets above the dim layer and let them take the mouse, or put them back.
        let topmost = on || self.options.topmost;
        for slot in &mut self.slots {
            slot.surface.set_topmost(topmost);
        }
        self.sync_interactive();
        self.sync_visibility();
        self.draw(None);
        self.sync_timers();
        self.sync_edit_surfaces();
    }

    /// Positions and redraws the selection outline, the guides and the hint.
    fn sync_edit_surfaces(&mut self) {
        let Some(gpu) = self.gpu.clone() else { return };
        if self.edit.is_none() {
            return;
        }
        let paint = self.paint.clone();
        let locked = self.layout.locked;

        // Selection outline around the selected widget.
        let selected = self.selected.as_ref().and_then(|id| self.placement(id)).cloned();
        let selection_target = selected.as_ref().map(|placement| {
            let area = self.area_index(placement.monitor.as_deref());
            let scale = self.areas.get(area).map_or(1.0, |a| a.scale);
            let (x, y) = self.to_physical(area, placement.x - edit::CHROME_SIDE, placement.y - edit::CHROME_TOP);
            let size = (placement.width + 2.0 * edit::CHROME_SIDE, placement.height + edit::CHROME_TOP + edit::CHROME_SIDE);
            let radius = self.widget_paint(placement).radius;
            (x, y, size, scale, (placement.width, placement.height), radius)
        });

        // Guides, if a drag has lined up with something.
        let guide_area = self.area_index(self.guides.monitor.as_deref());
        let guide_x = self.guides.x.map(|x| {
            let (px, py) = self.to_physical(guide_area, x - edit::GUIDE_THICKNESS / 2.0, 0.0);
            (px, py, (edit::GUIDE_THICKNESS, self.area_size(guide_area).1))
        });
        let guide_y = self.guides.y.map(|y| {
            let (px, py) = self.to_physical(guide_area, 0.0, y - edit::GUIDE_THICKNESS / 2.0);
            (px, py, (self.area_size(guide_area).0, edit::GUIDE_THICKNESS))
        });

        // The hint sits at the bottom centre of the primary monitor.
        let primary = self.area_index(None);
        let (area_width, area_height) = self.area_size(primary);
        let hint_at = self.to_physical(primary, (area_width - edit::HINT_SIZE.0) / 2.0, area_height - edit::HINT_SIZE.1 - 32.0);

        let Some(edit) = self.edit.as_mut() else {
            return;
        };
        let overlay = |x: i32, y: i32, size: (f32, f32)| SurfaceOptions { x, y, width: size.0, height: size.1, click_through: true, topmost: true };

        match selection_target {
            Some((x, y, size, scale, widget_size, radius)) => {
                // A surface made for another monitor's scale would be blurry: start again.
                if edit.selection.as_ref().is_some_and(|s| (s.scale() - scale).abs() > 0.001) {
                    edit.selection = None;
                }
                if edit.selection.is_none() {
                    edit.selection = Surface::new(&gpu, overlay(x, y, size)).ok();
                }
                if let Some(surface) = edit.selection.as_mut() {
                    surface.set_position(x, y);
                    let _ = surface.resize(size.0, size.1);
                    surface.set_topmost(true);
                    let _ = surface.draw(|canvas| edit::draw_selection(canvas, &paint, widget_size, radius));
                }
            }
            None => edit.selection = None,
        }

        for (slot, target) in [(&mut edit.guide_x, guide_x), (&mut edit.guide_y, guide_y)] {
            match target {
                Some((x, y, size)) => {
                    if slot.is_none() {
                        *slot = Surface::new(&gpu, overlay(x, y, size)).ok();
                    }
                    if let Some(surface) = slot.as_mut() {
                        surface.set_position(x, y);
                        let _ = surface.resize(size.0, size.1);
                        let _ = surface.draw(|canvas| edit::draw_guide(canvas, &paint, size));
                    }
                }
                None => *slot = None,
            }
        }

        if edit.hint.is_none() {
            edit.hint = Surface::new(&gpu, overlay(hint_at.0, hint_at.1, edit::HINT_SIZE)).ok();
        }
        if let Some(hint) = edit.hint.as_ref() {
            let _ = hint.draw(|canvas| edit::draw_hint(canvas, &paint, edit::HINT_SIZE, locked));
        }
    }

    // ----- dragging -----

    /// Which corner of the widget a point (physical pixels from its top-left) is on, if any.
    fn corner_at(slot: &Slot, x: i32, y: i32) -> Option<(bool, bool)> {
        let scale = slot.surface.scale();
        let (width, height) = slot.surface.size();
        let (x, y) = (x as f32 / scale, y as f32 / scale);
        let near_left = x <= edit::HANDLE_REACH;
        let near_right = x >= width - edit::HANDLE_REACH;
        let near_top = y <= edit::HANDLE_REACH;
        let near_bottom = y >= height - edit::HANDLE_REACH;
        ((near_left || near_right) && (near_top || near_bottom)).then_some((near_right, near_bottom))
    }

    fn on_mouse_down(&mut self, window: isize, x: i32, y: i32) {
        if self.edit.is_none() {
            return;
        }
        let Some(slot) = self.slots.iter().find(|slot| slot.surface.window() == window) else {
            return;
        };
        let id = slot.id.clone();
        let corner = Self::corner_at(slot, x, y);
        let Some(placement) = self.placement(&id).cloned() else {
            return;
        };

        if self.selected.as_deref() != Some(id.as_str()) {
            self.selected = Some(id.clone());
            self.sync_edit_surfaces();
            self.report();
        }
        if self.layout.locked {
            return;
        }
        let (mode, anchor) = match corner {
            Some((right, bottom)) => {
                // The opposite corner stays where it is.
                let anchor = (if right { placement.x } else { placement.x + placement.width }, if bottom { placement.y } else { placement.y + placement.height });
                (DragMode::Resize(right, bottom), anchor)
            }
            None => (DragMode::Move, (0.0, 0.0)),
        };
        unsafe { SetCapture(HWND(window as *mut _)) };
        if let Some(edit) = self.edit.as_mut() {
            edit.drag = Some(Drag { id, mode, grab: (x, y), anchor, start: (placement.width, placement.height, placement.scale), last_report: Instant::now() - DRAG_REPORT_EVERY });
        }
    }

    /// `x`, `y`: the mouse, in physical pixels from the dragged window's top-left.
    fn on_mouse_move(&mut self, window: isize, x: i32, y: i32) {
        let Some(drag) = self.edit.as_ref().and_then(|edit| edit.drag.as_ref()) else {
            return;
        };
        let (id, mode, grab, anchor, start) = (drag.id.clone(), drag.mode, drag.grab, drag.anchor, drag.start);
        let Some(slot) = self.slots.iter().find(|slot| slot.surface.window() == window && slot.id == id) else {
            return;
        };
        let (window_x, window_y) = slot.surface.position();
        let (screen_x, screen_y) = (window_x + x, window_y + y);
        let Some(placement) = self.placement(&id).cloned() else {
            return;
        };
        let current_area = self.area_index(placement.monitor.as_deref());

        let mut params = UpdateWidgetParams { id: id.clone(), dragging: true, ..Default::default() };
        match mode {
            DragMode::Move => {
                // The monitor under the mouse decides where the widget lives.
                let area = self.area_at(screen_x, screen_y).unwrap_or(current_area);
                let Some(target) = self.areas.get(area) else {
                    return;
                };
                params.x = Some((screen_x - grab.0 - target.work.0) as f32 / target.scale);
                params.y = Some((screen_y - grab.1 - target.work.1) as f32 / target.scale);
                if area != current_area {
                    params.monitor = Some(target.info.id.clone());
                }
            }
            DragMode::Resize(right, bottom) => {
                let Some(area) = self.areas.get(current_area) else {
                    return;
                };
                let pointer = ((screen_x - area.work.0) as f32 / area.scale, (screen_y - area.work.1) as f32 / area.scale);
                let (width, height) = snap::snap_size((pointer.0 - anchor.0).abs(), (pointer.1 - anchor.1).abs(), self.layout.snap);
                let (mut width, mut height) = (width.max(MIN_SIZE.0), height.max(MIN_SIZE.1));
                // With Shift held the contents grow and shrink with the box, which keeps its shape.
                if unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0 && start.0 > 0.0 {
                    let limits = lumen_ipc::widgets::SCALE_RANGE;
                    let ratio = ((pointer.0 - anchor.0).abs() / start.0).max((pointer.1 - anchor.1).abs() / start.1).clamp(limits.0 / start.2, limits.1 / start.2);
                    (width, height) = ((start.0 * ratio).round().max(MIN_SIZE.0), (start.1 * ratio).round().max(MIN_SIZE.1));
                    params.scale = Some(start.2 * ratio);
                    if let Some(widget) = self.layout.widgets.iter_mut().find(|w| w.id == id) {
                        widget.scale = (start.2 * ratio).clamp(limits.0, limits.1);
                    }
                }
                params.width = Some(width);
                params.height = Some(height);
                // Dragging a left or top handle moves that edge; the anchor corner stays.
                if !right || !bottom {
                    self.resize_from_anchor(&id, anchor, (width, height), (right, bottom));
                    return;
                }
            }
        }
        self.drag_update(params);
    }

    /// Resizing by a left or top handle: the size changes and the position
    /// moves so the opposite corner stays fixed. Done directly, without the
    /// move-snapping that would pull the anchored corner off its spot.
    fn resize_from_anchor(&mut self, id: &str, anchor: (f32, f32), size: (f32, f32), (right, bottom): (bool, bool)) {
        let Some(index) = self.layout.widgets.iter().position(|w| w.id == id) else {
            return;
        };
        let area = self.area_index(self.layout.widgets[index].monitor.as_deref());
        let (area_width, area_height) = self.area_size(area);
        let mut placement = self.layout.widgets[index].clone();
        placement.width = size.0.min(if right { area_width - anchor.0 } else { anchor.0 }).max(MIN_SIZE.0);
        placement.height = size.1.min(if bottom { area_height - anchor.1 } else { anchor.1 }).max(MIN_SIZE.1);
        placement.x = if right { anchor.0 } else { (anchor.0 - placement.width).max(0.0) };
        placement.y = if bottom { anchor.1 } else { (anchor.1 - placement.height).max(0.0) };
        if placement == self.layout.widgets[index] {
            return;
        }
        self.layout.widgets[index] = placement.clone();
        self.apply_placement(&placement, false);
        self.sync_edit_surfaces();
        self.throttled_report();
    }

    fn drag_update(&mut self, params: UpdateWidgetParams) {
        let Some(index) = self.layout.widgets.iter().position(|w| w.id == params.id) else {
            return;
        };
        let before = (self.layout.widgets[index].clone(), self.guides.clone());
        // Reuse the same path as updates from the settings app, minus its report.
        let report = std::mem::replace(&mut self.on_state, Box::new(|_| {}));
        self.update_widget(params);
        self.on_state = report;
        if before != (self.layout.widgets[index].clone(), self.guides.clone()) {
            self.throttled_report();
        }
    }

    fn throttled_report(&mut self) {
        let due = self.edit.as_mut().and_then(|edit| edit.drag.as_mut()).is_some_and(|drag| {
            let due = drag.last_report.elapsed() >= DRAG_REPORT_EVERY;
            if due {
                drag.last_report = Instant::now();
            }
            due
        });
        if due {
            self.report();
        }
    }

    fn on_mouse_up(&mut self) {
        let dragged = self.edit.as_mut().and_then(|edit| edit.drag.take());
        unsafe {
            let _ = ReleaseCapture();
        }
        if dragged.is_some() {
            // The drop is final: clear the guides, save, and tell the app.
            self.after_layout_change(true);
        }
    }

    /// The monitor containing a screen point (physical pixels).
    fn area_at(&self, x: i32, y: i32) -> Option<usize> {
        self.areas.iter().position(|a| x >= a.bounds.0 && y >= a.bounds.1 && x < a.bounds.0 + a.bounds.2 && y < a.bounds.1 + a.bounds.3)
    }

    /// The mouse pointer for a spot on a widget, in edit mode.
    fn cursor_for(&self, window: isize) -> Option<windows::core::PCWSTR> {
        self.edit.as_ref()?;
        let slot = self.slots.iter().find(|slot| slot.surface.window() == window)?;
        if self.layout.locked {
            return Some(IDC_ARROW);
        }
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.ok()?;
        let (x, y) = slot.surface.position();
        Some(match Self::corner_at(slot, cursor.x - x, cursor.y - y) {
            Some((right, bottom)) if right == bottom => IDC_SIZENWSE,
            Some(_) => IDC_SIZENESW,
            None => IDC_SIZEALL,
        })
    }
}

/// Reads the connected monitors: ids and wallpapers from the desktop
/// wallpaper service, work areas and scales from the display system.
fn read_areas() -> Vec<Area> {
    let monitors = lumen_win::wallpaper::Wallpapers::new().and_then(|w| w.monitors()).unwrap_or_default();
    let mut areas: Vec<Area> = monitors
        .into_iter()
        .filter_map(|m| unsafe {
            let rect = RECT { left: m.x, top: m.y, right: m.x + m.width, bottom: m.y + m.height };
            let (work, bounds, scale) = monitor_metrics(MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST))?;
            Some(Area {
                info: MonitorArea {
                    id: m.id,
                    number: m.number,
                    primary: m.primary,
                    width: work.2 as f32 / scale,
                    height: work.3 as f32 / scale,
                    wallpaper: m.wallpaper.map(|p| p.display().to_string()),
                },
                work: (work.0, work.1),
                work_rect: work,
                bounds,
                scale,
            })
        })
        .collect();

    if areas.is_empty() {
        // The wallpaper service was unavailable: fall back to the primary monitor alone.
        if let Some((work, bounds, scale)) = unsafe { monitor_metrics(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)) } {
            let info = MonitorArea { id: String::new(), number: 1, primary: true, width: work.2 as f32 / scale, height: work.3 as f32 / scale, wallpaper: None };
            areas.push(Area { info, work: (work.0, work.1), work_rect: work, bounds, scale });
        }
    }
    areas
}

type Metrics = ((i32, i32, i32, i32), (i32, i32, i32, i32), f32);

/// `(work area, whole monitor, scale)` with rectangles as `(x, y, width, height)` in physical pixels.
unsafe fn monitor_metrics(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<Metrics> {
    unsafe {
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let rect = |r: RECT| (r.left, r.top, r.right - r.left, r.bottom - r.top);
        Some((rect(info.rcWork), rect(info.rcMonitor), dpi_x.max(96) as f32 / 96.0))
    }
}

/// Clicking the dimmed desktop clears the selection.
fn on_dim_click() {
    with_host(|host| host.run(Command::Select(None)));
}

/// Mouse input on a widget window (only delivered in edit mode).
fn on_input(window: isize, message: u32, wparam: usize, lparam: isize) -> Option<isize> {
    // Client coordinates are packed as two signed 16-bit numbers.
    let x = i32::from((lparam & 0xFFFF) as u16 as i16);
    let y = i32::from(((lparam >> 16) & 0xFFFF) as u16 as i16);
    // The launcher is the one surface that takes typing; all its messages go to it.
    if with_host(|host| host.is_launcher_window(window)) == Some(true) {
        if matches!(message, WM_CHAR | WM_KEYDOWN | WM_ACTIVATE | WM_LBUTTONUP) {
            with_host(|host| host.on_launcher_message(message, wparam, y));
            return Some(0);
        }
        return None;
    }
    match message {
        WM_LBUTTONDOWN => with_host(|host| host.on_mouse_down(window, x, y)).map(|()| 0),
        WM_MOUSEMOVE => with_host(|host| host.on_mouse_move(window, x, y)).map(|()| 0),
        WM_LBUTTONUP => with_host(|host| {
            if host.is_bar_window(window) {
                host.on_bar_click(window, x);
            } else if host.edit.is_none() {
                host.on_widget_click(window, x, y);
            } else {
                host.on_mouse_up();
            }
        })
        .map(|()| 0),
        WM_CAPTURECHANGED => with_host(Host::on_mouse_up).map(|()| 0),
        WM_SETCURSOR => {
            let cursor = with_host(|host| {
                if host.is_bar_window(window) {
                    // A pointing hand over anything on the bar that can be clicked.
                    let mut cursor = POINT::default();
                    let _ = unsafe { GetCursorPos(&mut cursor) };
                    let left = host.bars_left_edge(window).unwrap_or(0);
                    Some(if host.bar_hot(window, cursor.x - left) { IDC_HAND } else { IDC_ARROW })
                } else if host.edit.is_none() {
                    // A pointing hand over a part that does something; a plain arrow elsewhere on the widget.
                    let mut cursor = POINT::default();
                    let _ = unsafe { GetCursorPos(&mut cursor) };
                    let origin = host.slots.iter().find(|slot| slot.surface.window() == window).map(|slot| slot.surface.position())?;
                    Some(if host.widget_action(window, cursor.x - origin.0, cursor.y - origin.1).is_some() { IDC_HAND } else { IDC_ARROW })
                } else {
                    host.cursor_for(window)
                }
            })
            .flatten()?;
            unsafe { SetCursor(LoadCursorW(None, cursor).ok()) };
            Some(1)
        }
        _ => None,
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => {
            with_host(|host| host.on_timer(wparam.0));
        }
        WAKE => {
            with_host(Host::on_wake);
        }
        WM_HOTKEY => {
            with_host(|host| {
                let on = match wparam.0 as i32 {
                    HOTKEY_EDIT => host.edit.is_none(),
                    HOTKEY_ESCAPE => false,
                    launchers::HOTKEY_LAUNCHER => return host.toggle_launcher(),
                    key @ (flyouts::HOTKEY_VOLUME_UP | flyouts::HOTKEY_VOLUME_DOWN | flyouts::HOTKEY_VOLUME_MUTE) => return host.on_volume_key(key),
                    _ => return,
                };
                host.run(Command::EditMode(on));
            });
        }
        // The clock was changed by hand or by a time sync.
        WM_TIMECHANGE => {
            with_host(|host| host.on_timer(TIMER_CLOCK));
        }
        // Monitors were added, removed, rearranged or rescaled.
        // Something that holds screen space (the taskbar, another bar) moved.
        files::FILES_CHANGED => {
            with_host(Host::on_files_changed);
        }
        flyouts::VOLUME_CHANGED => {
            with_host(Host::on_volume_changed);
        }
        BAR_CALLBACK => {
            if wparam.0 == lumen_win::appbar::POSITION_CHANGED {
                with_host(Host::reposition_bars);
            }
        }
        WM_DISPLAYCHANGE => {
            with_host(|host| {
                if let Err(error) = host.rebuild() {
                    warn!("could not move the widgets after a display change: {error}");
                }
                // The monitors changed, so the bars need new places too.
                host.place_bars();
                host.draw(None);
                host.sync_timers();
                host.report();
            });
        }
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
}

/// Opens the Lumen app, which sits next to the core.
pub(super) fn open_settings_app() {
    let opened = std::env::current_exe().ok().map(|exe| exe.with_file_name("lumen-settings.exe")).filter(|app| app.is_file()).is_some_and(|app| lumen_win::shell::open(&app.to_string_lossy()));
    if !opened {
        info!("the settings app is not installed next to lumen-core");
    }
}

/// Reads the font files the user added to Lumen. A file that cannot be read
/// is skipped; one that is not a font is ignored by the graphics layer.
fn imported_fonts(dir: &std::path::Path) -> Vec<Vec<u8>> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_file()).collect();
    files.sort();
    files.into_iter().take(64).filter_map(|path| std::fs::read(path).ok()).collect()
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
