//! Runs the widgets: owns their windows, timers and data, and redraws a
//! widget only when what it shows has changed.
//!
//! The host lives on one thread (the one that calls [`start`]) and is driven
//! by that thread's message loop. Other threads reach it through a
//! [`HostHandle`], which drops a message in its inbox and wakes it.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use lumen_render::{Bitmap, Error as RenderError, Gpu, Surface, SurfaceOptions};
use lumen_theme::Theme;
use tracing::{info, warn};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, KillTimer, PostMessageW, RegisterClassW, SetTimer, WINDOW_STYLE, WM_APP, WM_DISPLAYCHANGE,
                WM_TIMECHANGE, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::w,
};

use crate::{
    BUNDLED_FONTS,
    clock::{self, ClockData},
    layout::{Kind, Layout, Placement},
    media::{self, MediaInfo},
    paint::Paint,
    stats::{self, StatsData, StatsSampler},
};

const WAKE: u32 = WM_APP + 10;
const TIMER_CLOCK: usize = 1;
const TIMER_STATS: usize = 2;
const TIMER_MEDIA: usize = 3;
/// How often the now-playing progress bar moves while something is playing.
const MEDIA_TICK_MS: u32 = 1000;

/// How to run the widgets.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Keep widgets above other windows (for testing; normally they sit on the desktop).
    pub topmost: bool,
}

/// Lets other threads talk to the host.
#[derive(Clone)]
pub struct HostHandle {
    window: isize,
    inbox: Arc<Mutex<Inbox>>,
}

#[derive(Default)]
struct Inbox {
    theme: Option<Theme>,
    /// `Some(None)` means "nothing is playing now".
    media: Option<Option<MediaInfo>>,
}

impl HostHandle {
    /// Recolours every widget with this theme.
    pub fn set_theme(&self, theme: Theme) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).theme = Some(theme);
        self.wake();
    }

    fn set_media(&self, media: Option<MediaInfo>) {
        self.inbox.lock().unwrap_or_else(|p| p.into_inner()).media = Some(media);
        self.wake();
    }

    fn wake(&self) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.window as *mut _)), WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

struct Slot {
    kind: Kind,
    surface: Surface,
}

struct Host {
    window: HWND,
    options: Options,
    layout: Layout,
    paint: Paint,
    paused: bool,
    inbox: Arc<Mutex<Inbox>>,

    gpu: Option<Rc<Gpu>>,
    slots: Vec<Slot>,

    clock: ClockData,
    sampler: StatsSampler,
    stats: StatsData,
    media: Option<MediaInfo>,
    /// The current track's cover, uploaded to the GPU on first use.
    art: Option<Bitmap>,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

/// Starts the widgets on this thread. The thread must run a message loop.
pub fn start(layout: Layout, theme: &Theme, options: Options) -> Result<HostHandle, String> {
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
        options,
        layout,
        paint: Paint::from_theme(theme),
        paused: false,
        inbox: inbox.clone(),
        gpu: None,
        slots: Vec::new(),
        clock: ClockData::now(),
        stats: sampler.sample(),
        sampler,
        media: None,
        art: None,
    };
    host.rebuild()?;
    host.start_timers();
    info!("{} widgets on the desktop", host.slots.len());
    HOST.set(Some(host));

    let handle = HostHandle { window: window.0 as isize, inbox };
    let for_media = handle.clone();
    media::watch(move |info| for_media.set_media(info));
    Ok(handle)
}

/// Pauses or resumes all drawing and timers (game mode). While paused the
/// widgets cost nothing; on resume they catch up at once.
pub fn set_paused(paused: bool) {
    with_host(|host| {
        if host.paused == paused {
            return;
        }
        host.paused = paused;
        if paused {
            host.stop_timers();
            for slot in &mut host.slots {
                slot.surface.set_visible(false);
            }
        } else {
            for slot in &mut host.slots {
                slot.surface.set_visible(true);
            }
            host.clock = ClockData::now();
            host.stats = host.sampler.sample();
            host.draw_all();
            host.start_timers();
        }
    });
}

/// Removes the widgets. Call on the host's thread before it exits.
pub fn stop() {
    if let Some(host) = HOST.take() {
        host.stop_timers();
        unsafe {
            let _ = DestroyWindow(host.window);
        }
    }
}

fn with_host(action: impl FnOnce(&mut Host)) {
    HOST.with(|cell| {
        // If the host is already busy (a nested message), skip rather than panic.
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(host) = guard.as_mut()
        {
            action(host);
        }
    });
}

impl Host {
    /// (Re)creates the graphics device and every widget window, then draws them.
    fn rebuild(&mut self) -> Result<(), String> {
        self.slots.clear();
        self.art = None;
        self.gpu = None;
        let gpu = Gpu::new(BUNDLED_FONTS).map_err(text)?;

        let (origin_x, origin_y, scale) = primary_work_area();
        for placement in &self.layout.widgets {
            let options = SurfaceOptions {
                x: origin_x + (placement.x * scale).round() as i32,
                y: origin_y + (placement.y * scale).round() as i32,
                width: placement.width,
                height: placement.height,
                // Widgets are display-only until edit mode (M6); clicks reach the desktop.
                click_through: true,
                topmost: self.options.topmost,
            };
            match Surface::new(&gpu, options) {
                Ok(surface) => self.slots.push(Slot { kind: placement.kind, surface }),
                Err(error) => warn!("could not create the widget {}: {error}", placement_name(placement)),
            }
        }
        self.gpu = Some(gpu);
        if self.paused {
            for slot in &mut self.slots {
                slot.surface.set_visible(false);
            }
        } else {
            self.draw_all();
        }
        Ok(())
    }

    fn draw_all(&mut self) {
        self.draw(None);
    }

    /// Redraws the widgets of one kind, or all of them.
    fn draw(&mut self, only: Option<Kind>) {
        if self.paused {
            return;
        }
        let mut lost = false;
        for index in 0..self.slots.len() {
            let kind = self.slots[index].kind;
            if only.is_some_and(|wanted| wanted != kind) {
                continue;
            }
            let (width, height) = self.slots[index].surface.size();
            let (paint, clock, stats, media) = (&self.paint, &self.clock, &self.stats, self.media.as_ref());
            let art = &mut self.art;
            let result = self.slots[index].surface.draw(|canvas| match kind {
                Kind::Clock => clock::draw(canvas, paint, clock, width, height),
                Kind::SystemStats => stats::draw(canvas, paint, stats, width, height),
                Kind::NowPlaying => {
                    if art.is_none()
                        && let Some(cover) = media.and_then(|m| m.art.as_ref())
                    {
                        *art = canvas.bitmap_from_rgba(cover.size, cover.size, &cover.rgba).ok();
                    }
                    media::draw(canvas, paint, media, art.as_ref(), width, height);
                }
            });
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
        }
    }

    fn start_timers(&self) {
        unsafe {
            SetTimer(Some(self.window), TIMER_CLOCK, clock::millis_until_next_minute(), None);
            if self.has(Kind::SystemStats) {
                SetTimer(Some(self.window), TIMER_STATS, stats::SAMPLE_INTERVAL_MS, None);
            }
        }
        self.sync_media_timer();
    }

    fn stop_timers(&self) {
        unsafe {
            for timer in [TIMER_CLOCK, TIMER_STATS, TIMER_MEDIA] {
                let _ = KillTimer(Some(self.window), timer);
            }
        }
    }

    /// The progress bar only needs a timer while a track is actually playing.
    fn sync_media_timer(&self) {
        let advancing = !self.paused && self.has(Kind::NowPlaying) && self.media.as_ref().is_some_and(MediaInfo::is_advancing);
        unsafe {
            if advancing {
                SetTimer(Some(self.window), TIMER_MEDIA, MEDIA_TICK_MS, None);
            } else {
                let _ = KillTimer(Some(self.window), TIMER_MEDIA);
            }
        }
    }

    fn has(&self, kind: Kind) -> bool {
        self.slots.iter().any(|slot| slot.kind == kind)
    }

    fn on_timer(&mut self, timer: usize) {
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
                let now = self.sampler.sample();
                if now != self.stats {
                    self.stats = now;
                    self.draw(Some(Kind::SystemStats));
                }
            }
            TIMER_MEDIA => self.draw(Some(Kind::NowPlaying)),
            _ => {}
        }
    }

    /// Handles what other threads left in the inbox.
    fn on_wake(&mut self) {
        let (theme, media) = {
            let mut inbox = self.inbox.lock().unwrap_or_else(|p| p.into_inner());
            (inbox.theme.take(), inbox.media.take())
        };
        if let Some(media) = media {
            let same_cover = match (&media, &self.media) {
                (Some(new), Some(old)) => new.title == old.title && new.artist == old.artist && new.art.is_some() == old.art.is_some(),
                _ => false,
            };
            if !same_cover {
                self.art = None;
            }
            self.media = media;
            self.sync_media_timer();
            if theme.is_none() {
                self.draw(Some(Kind::NowPlaying));
            }
        }
        if let Some(theme) = theme {
            let paint = Paint::from_theme(&theme);
            if paint != self.paint {
                self.paint = paint;
            }
            self.draw_all();
        }
    }
}

fn placement_name(placement: &Placement) -> &str {
    &placement.id
}

/// Top-left of the primary monitor's work area (physical pixels) and its scale.
fn primary_work_area() -> (i32, i32, f32) {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let (x, y) = if GetMonitorInfoW(monitor, &mut info).as_bool() { (info.rcWork.left, info.rcWork.top) } else { (0, 0) };
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        (x, y, dpi_x.max(96) as f32 / 96.0)
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => with_host(|host| host.on_timer(wparam.0)),
        WAKE => with_host(Host::on_wake),
        // The clock was changed by hand or by a time sync.
        WM_TIMECHANGE => with_host(|host| host.on_timer(TIMER_CLOCK)),
        // Monitors were added, removed, rearranged or rescaled.
        WM_DISPLAYCHANGE => with_host(|host| {
            if let Err(error) = host.rebuild() {
                warn!("could not move the widgets after a display change: {error}");
            }
        }),
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    LRESULT(0)
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
