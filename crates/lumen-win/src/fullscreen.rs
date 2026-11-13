//! Fullscreen app and game detection.
//!
//! Anti-cheat safety: this only asks Windows about the foreground window
//! (its rectangle, style, class and title) and listens to out-of-context
//! WinEvent hooks, whose callbacks run in Lumen's own process. It never
//! injects code into, or opens a handle to, the other process.

use std::{cell::RefCell, time::Duration};

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        Shell::{QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState},
        WindowsAndMessaging::{
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, GWL_STYLE, GetClassNameW, GetDesktopWindow, GetForegroundWindow,
            DispatchMessageW, IsZoomed, GetMessageW, TranslateMessage, GetShellWindow, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, KillTimer, MSG,
            OBJID_WINDOW, SetTimer, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_TIMER, WS_CAPTION,
        },
    },
};

use crate::{Context, Result};

/// Shell surfaces that can cover a whole monitor without being an app:
/// the desktop itself, the taskbar, and Task View / Alt+Tab.
const SHELL_CLASSES: [&str; 5] = ["Progman", "WorkerW", "Shell_TrayWnd", "XamlExplorerHostIslandWindow", "MultitaskingViewFrame"];

/// How the app is fullscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Exclusive Direct3D fullscreen (most older games).
    Exclusive,
    /// A borderless window covering the whole monitor (most current games,
    /// video players, browsers in F11).
    Borderless,
}

/// The fullscreen app currently in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullscreenApp {
    pub kind: Kind,
    pub title: String,
    pub window_class: String,
    /// Top-left corner of the monitor it covers, on the virtual desktop.
    pub monitor_x: i32,
    pub monitor_y: i32,
    window: isize,
}

/// Returns the fullscreen app in the foreground, or `None` on a normal desktop.
pub fn current() -> Option<FullscreenApp> {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() || window == GetShellWindow() || window == GetDesktopWindow() {
            return None;
        }
        let window_class = class_name(window);
        if SHELL_CLASSES.contains(&window_class.as_str()) {
            return None;
        }

        let notification_state = SHQueryUserNotificationState().ok();
        let monitor = monitor_rect(window)?;
        let covers = window_rect(window).is_some_and(|w| {
            w.left <= monitor.left && w.top <= monitor.top && w.right >= monitor.right && w.bottom >= monitor.bottom
        });
        let has_title_bar = GetWindowLongW(window, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0;

        let maximised = IsZoomed(window).as_bool();

        // Windows' own verdict comes first: it is what hides the taskbar and
        // holds back notifications. It can lag a moment behind the window
        // itself, which is why the watcher re-checks shortly after any change.
        let kind = if notification_state == Some(QUNS_RUNNING_D3D_FULL_SCREEN) {
            Kind::Exclusive
        } else if covers && notification_state == Some(QUNS_BUSY) {
            Kind::Borderless
        } else if covers && !has_title_bar && !maximised {
            // Not flagged by Windows, but a bare window exactly covering the
            // monitor. A maximised window is excluded: on a monitor without a
            // taskbar it also covers everything, and it is not fullscreen.
            Kind::Borderless
        } else {
            return None;
        };

        Some(FullscreenApp {
            kind,
            title: window_text(window),
            window_class,
            monitor_x: monitor.left,
            monitor_y: monitor.top,
            window: window.0 as isize,
        })
    }
}

/// Calls `on_change` whenever a fullscreen app appears (`Some`) or goes away
/// (`None`), starting with the current state. Blocks the calling thread,
/// pumping its message queue, for `duration` or until the thread gets WM_QUIT.
///
/// Event-driven: on the desktop it wakes only when the foreground window
/// changes, moves or resizes. While a fullscreen app is in front it listens
/// only for the foreground changing, plus one cheap re-check every 5 seconds.
pub fn watch(duration: Option<Duration>, on_change: impl FnMut(Option<&FullscreenApp>) + 'static) -> Result<()> {
    WATCH.with_borrow_mut(|watch| *watch = Some(Watch { last: None, on_change: Box::new(on_change), location_hook: None, settle_timer: 0, game_timer: 0 }));

    let result = unsafe {
        let foreground_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if foreground_hook.is_invalid() {
            Err(windows::core::Error::from_thread()).context("listening for foreground changes")
        } else {
            follow_foreground_window();
            report(When::Start);

            let timer = duration.map(|d| SetTimer(None, 0, d.as_millis().try_into().unwrap_or(u32::MAX), None));
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                if message.message == WM_TIMER && message.hwnd.is_invalid() {
                    if timer == Some(message.wParam.0) {
                        break;
                    }
                    let (settle, game) = timers();
                    if settle == message.wParam.0 {
                        let _ = KillTimer(None, message.wParam.0);
                        report(When::Settled);
                        continue;
                    }
                    if game == message.wParam.0 {
                        // Still in the game? (The timer keeps running until it is not.)
                        report(When::Settled);
                        continue;
                    }
                }
                // Deliver window messages too, so the caller's own windows (the tray) work on this thread.
                // Key presses become typed characters for a window that has the keyboard (the launcher).
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if let Some(timer) = timer {
                let _ = KillTimer(None, timer);
            }
            let _ = UnhookWinEvent(foreground_hook);
            Ok(())
        }
    };

    if let Some(Watch { location_hook: Some(hook), .. }) = WATCH.take() {
        unsafe {
            let _ = UnhookWinEvent(hook);
        }
    }
    result
}

type Callback = Box<dyn FnMut(Option<&FullscreenApp>)>;

struct Watch {
    last: Option<FullscreenApp>,
    on_change: Callback,
    /// Move/resize hook, scoped to the foreground window's thread so that
    /// mouse movement and background windows never wake us.
    location_hook: Option<HWINEVENTHOOK>,
    /// One-shot timer for the settle check; 0 when none has been set yet.
    settle_timer: usize,
    /// Repeating timer that re-checks while a game is in front; 0 otherwise.
    game_timer: usize,
}

/// How long after the latest change to look again. Windows' fullscreen flag
/// and the window's own size do not always change in the same instant.
const SETTLE_MS: u32 = 500;

#[derive(Clone, Copy, PartialEq, Eq)]
enum When {
    /// The first report: always made.
    Start,
    /// Straight after a window event: "off" is reported now, "on" waits to be confirmed.
    Event,
    /// The settle check: whatever is true now is reported.
    Settled,
}

/// How often to re-check while a game is in front. This is the only thing the
/// watcher does during a game besides waiting for the foreground to change.
const GAME_RECHECK_MS: u32 = 5000;

/// The ids of the settle timer and the in-game re-check timer (0 = not set).
fn timers() -> (usize, usize) {
    WATCH.with_borrow(|watch| watch.as_ref().map_or((0, 0), |w| (w.settle_timer, w.game_timer)))
}

thread_local! {
    // WinEvent callbacks carry no user data, so the watcher's state lives here.
    static WATCH: RefCell<Option<Watch>> = const { RefCell::new(None) };
}

unsafe extern "system" fn on_event(_hook: HWINEVENTHOOK, event: u32, window: HWND, object: i32, _child: i32, _thread: u32, _time: u32) {
    if event == EVENT_SYSTEM_FOREGROUND {
        follow_foreground_window();
        report(When::Event);
    } else if event == EVENT_OBJECT_LOCATIONCHANGE && object == OBJID_WINDOW.0 && window == unsafe { GetForegroundWindow() } {
        report(When::Event);
    }
}

/// Re-points the move/resize hook at the thread that owns the foreground
/// window. Does nothing while a game is in front (see [`report`]).
fn follow_foreground_window() {
    if WATCH.with_borrow(|watch| watch.as_ref().is_some_and(|w| w.last.is_some())) {
        return;
    }
    let hook = unsafe {
        let mut process = 0;
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process));
        (thread != 0)
            .then(|| {
                SetWinEventHook(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    None,
                    Some(on_event),
                    process,
                    thread,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            })
            .filter(|hook| !hook.is_invalid())
    };
    let previous = WATCH.with_borrow_mut(|watch| watch.as_mut().and_then(|w| std::mem::replace(&mut w.location_hook, hook)));
    if let Some(previous) = previous {
        unsafe {
            let _ = UnhookWinEvent(previous);
        }
    }
}

/// Re-checks the fullscreen state and calls the callback if it changed.
///
/// Leaving fullscreen is reported at once. Entering it is reported only by
/// the settle check, so a stale reading (Windows still saying "fullscreen"
/// for a moment after a game closes) never pauses the desktop by mistake.
fn report(when: When) {
    let now = current();
    // Take the callback out while it runs so it may safely pump messages.
    let Some(mut watch) = WATCH.take() else { return };
    let changed = now.as_ref().map(|app| app.window) != watch.last.as_ref().map(|app| app.window);
    let fire = match when {
        When::Start => true,
        When::Event => changed && now.is_none(),
        When::Settled => changed,
    };
    let mut left_game = false;
    if fire {
        (watch.on_change)(now.as_ref());
        let was_in_game = watch.last.is_some();
        watch.last = now;
        let in_game = watch.last.is_some();
        if in_game && !was_in_game {
            // While a game is in front, stop listening to its window's
            // move/resize events: a busy game can produce a stream of them,
            // and each one would wake this thread. A slow re-check takes over,
            // for the rare game that leaves fullscreen without losing focus.
            if let Some(hook) = watch.location_hook.take() {
                unsafe {
                    let _ = UnhookWinEvent(hook);
                }
            }
            watch.game_timer = unsafe { SetTimer(None, watch.game_timer, GAME_RECHECK_MS, None) };
        } else if !in_game && was_in_game {
            if watch.game_timer != 0 {
                unsafe {
                    let _ = KillTimer(None, watch.game_timer);
                }
                watch.game_timer = 0;
            }
            left_game = true;
        }
    }
    if when != When::Settled {
        // (Re)start the one-shot settle timer; passing its id back replaces it.
        watch.settle_timer = unsafe { SetTimer(None, watch.settle_timer, SETTLE_MS, None) };
    }
    WATCH.set(Some(watch));
    if left_game {
        // Back on the desktop: listen to the foreground window again.
        follow_foreground_window();
    }
}

fn window_rect(window: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window, &mut rect) }.ok().map(|()| rect)
}

fn monitor_rect(window: HWND) -> Option<RECT> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut info) }.as_bool().then_some(info.rcMonitor)
}

fn class_name(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..len])
}

/// For another process's window this reads the caption Windows already holds;
/// it does not send the window a message.
fn window_text(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetWindowTextW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..len])
}
