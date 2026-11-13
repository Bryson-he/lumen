//! The notification-area (tray) icon and its menu.
//!
//! Plain Win32 on the main thread: a hidden window receives the icon's
//! messages. The menu is the basic one for now; preset switching, "pause all"
//! and edit mode join it in later milestones.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::{
                AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
                GetCursorPos, HICON, LR_DEFAULTCOLOR, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_STYLE,
                WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_ENDSESSION, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{PCWSTR, w},
};

/// The 32 px app icon, shared with the settings app.
const ICON_PNG: &[u8] = include_bytes!("../../../apps/settings/src-tauri/icons/32x32.png");

const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;
const MENU_OPEN: usize = 1;
const MENU_QUIT: usize = 2;

/// Explorer broadcasts this when the taskbar is (re)created, e.g. after it
/// restarts; the icon must then be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// The icon handle, for the window procedure to re-add it.
static ICON: AtomicIsize = AtomicIsize::new(0);

/// Shows the tray icon for as long as it lives. Its messages are delivered by
/// the message loop of the thread that created it.
pub struct Tray {
    window: HWND,
    icon: HICON,
}

impl Tray {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(text)?;
            let class = w!("LumenCoreTray");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
            // A hidden top-level window (not message-only), so it also receives
            // the "TaskbarCreated" broadcast.
            let window = CreateWindowExW(WS_EX_TOOLWINDOW, class, w!("Lumen"), WINDOW_STYLE(0), 0, 0, 0, 0, None, None, Some(instance.into()), None)
                .map_err(text)?;
            TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

            let icon = CreateIconFromResourceEx(ICON_PNG, true, 0x0003_0000, 32, 32, LR_DEFAULTCOLOR).map_err(text)?;
            ICON.store(icon.0 as isize, Ordering::Relaxed);
            let tray = Self { window, icon };
            tray.add();
            Ok(tray)
        }
    }

    fn add(&self) {
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data(self.window, self.icon)) }.as_bool();
        if added {
            tracing::info!("tray icon added");
        } else {
            tracing::warn!("Windows did not accept the tray icon");
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(self.window, self.icon));
            let _ = DestroyWindow(self.window);
            let _ = DestroyIcon(self.icon);
        }
    }
}

fn icon_data(window: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip("Lumen".encode_utf16()) {
        *slot = unit;
    }
    data
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if message == TRAY_MESSAGE {
            match lparam.0 as u32 {
                WM_LBUTTONUP => open_settings(),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(window),
                _ => {}
            }
            return LRESULT(0);
        }
        if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) {
            let icon = HICON(ICON.load(Ordering::Relaxed) as *mut _);
            let _ = Shell_NotifyIconW(NIM_ADD, &icon_data(window, icon));
            return LRESULT(0);
        }
        // Asked to close (an installer, an updater, Windows signing out): leave
        // the message loop so the core shuts down cleanly and reverts any preview.
        if message == WM_CLOSE || (message == WM_ENDSESSION && wparam.0 != 0) {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

unsafe fn show_menu(window: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let open_flags = if settings_exe().is_some() { MF_STRING } else { MF_STRING | MF_GRAYED };
        let _ = AppendMenuW(menu, open_flags, MENU_OPEN, w!("Open Lumen"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit Lumen"));

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        // Needed so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(window);
        let choice = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, cursor.x, cursor.y, None, window, None);
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        match choice.0 as usize {
            MENU_OPEN => open_settings(),
            MENU_QUIT => PostQuitMessage(0),
            _ => {}
        }
    }
}

/// The settings app sits next to `lumen-core.exe`.
fn settings_exe() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?.with_file_name("lumen-settings.exe");
    exe.is_file().then_some(exe)
}

fn open_settings() {
    match settings_exe() {
        Some(exe) => {
            if let Err(error) = std::process::Command::new(&exe).spawn() {
                tracing::warn!("could not start {}: {error}", exe.display());
            }
        }
        None => tracing::info!("the settings app is not installed next to lumen-core"),
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
