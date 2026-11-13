//! The notification-area (tray) icon and its menu.
//!
//! Plain Win32 on the main thread: a hidden window receives the icon's
//! messages. The menu offers quick preset switching, the widget switches
//! (edit, pause), opening the settings app, and quitting.

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicIsize, AtomicU32, Ordering},
};

use lumen_ipc::PresetInfo;
use lumen_widgets::host;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW},
            WindowsAndMessaging::{
                AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
                GetCursorPos, HICON, LR_DEFAULTCOLOR, MF_CHECKED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_STYLE,
                WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_ENDSESSION, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{HSTRING, PCWSTR, w},
};

/// The 32 px app icon, shared with the settings app.
const ICON_PNG: &[u8] = include_bytes!("../../../apps/settings/src-tauri/icons/32x32.png");

const TRAY_MESSAGE: u32 = WM_APP + 1;
/// Posting this to the tray window with a menu id in `wParam` runs that menu command.
const TRAY_COMMAND: u32 = WM_APP + 2;
const TRAY_ID: u32 = 1;
const MENU_OPEN: usize = 1;
const MENU_QUIT: usize = 2;
const MENU_EDIT: usize = 3;
const MENU_PAUSE: usize = 4;
/// Presets take the ids from here up, in list order.
const MENU_PRESET_FIRST: usize = 100;
const MAX_MENU_PRESETS: usize = 10;

/// What the tray needs from the rest of the core.
pub struct Controls {
    /// The preset list, kept current by the service.
    pub presets: Arc<Mutex<Vec<PresetInfo>>>,
    /// Applies a preset by id.
    pub apply_preset: Box<dyn Fn(String) + Send + Sync>,
}

static CONTROLS: OnceLock<Controls> = OnceLock::new();

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
    pub fn new(controls: Controls) -> Result<Self, String> {
        let _ = CONTROLS.set(controls);
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
        // The same commands as the menu, without the menu (scripts and tests).
        if message == TRAY_COMMAND {
            run_command(wparam.0);
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

/// One line of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Entry { id: usize, label: String, checked: bool, enabled: bool },
    Separator,
}

/// What the menu shows: presets to switch to, the widget switches, then the app.
fn menu_items(presets: &[PresetInfo], editing: bool, paused: bool, settings_installed: bool) -> Vec<Item> {
    let entry = |id: usize, label: &str, checked: bool, enabled: bool| Item::Entry { id, label: label.to_owned(), checked, enabled };
    let mut items = Vec::new();
    if presets.is_empty() {
        items.push(entry(0, "No presets yet", false, false));
    }
    for (index, preset) in presets.iter().take(MAX_MENU_PRESETS).enumerate() {
        // "&" would otherwise be read as a keyboard shortcut marker.
        items.push(entry(MENU_PRESET_FIRST + index, &preset.name.replace('&', "&&"), preset.active, true));
    }
    items.push(Item::Separator);
    items.push(entry(MENU_EDIT, "Edit widgets\tWin+Alt+E", editing, !paused));
    items.push(entry(MENU_PAUSE, "Pause widgets", paused, true));
    items.push(Item::Separator);
    items.push(entry(MENU_OPEN, "Open Lumen", false, settings_installed));
    items.push(entry(MENU_QUIT, "Quit Lumen", false, true));
    items
}

fn current_presets() -> Vec<PresetInfo> {
    CONTROLS.get().map(|controls| controls.presets.lock().unwrap_or_else(|p| p.into_inner()).clone()).unwrap_or_default()
}

unsafe fn show_menu(window: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        for item in menu_items(&current_presets(), host::is_editing(), host::is_user_paused(), settings_exe().is_some()) {
            match item {
                Item::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Item::Entry { id, label, checked, enabled } => {
                    let mut flags = MF_STRING;
                    if checked {
                        flags |= MF_CHECKED;
                    }
                    if !enabled {
                        flags |= MF_GRAYED;
                    }
                    let label = HSTRING::from(label);
                    let _ = AppendMenuW(menu, flags, id, PCWSTR(label.as_ptr()));
                }
            }
        }

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        // Needed so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(window);
        let choice = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, cursor.x, cursor.y, None, window, None);
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        run_command(choice.0 as usize);
    }
}

/// Carries out a menu choice.
fn run_command(id: usize) {
    match id {
        MENU_OPEN => open_settings(),
        MENU_QUIT => unsafe { PostQuitMessage(0) },
        MENU_EDIT => host::set_edit_mode(!host::is_editing()),
        MENU_PAUSE => host::set_user_paused(!host::is_user_paused()),
        id if id >= MENU_PRESET_FIRST => {
            if let (Some(preset), Some(controls)) = (current_presets().get(id - MENU_PRESET_FIRST), CONTROLS.get()) {
                (controls.apply_preset)(preset.id.clone());
            }
        }
        _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, name: &str, active: bool) -> PresetInfo {
        PresetInfo { id: id.into(), name: name.into(), thumb: None, palette: Vec::new(), created_at: 0, last_used_at: 0, widget_count: 0, active }
    }

    fn labels(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Separator => "---".to_owned(),
                Item::Entry { label, checked, enabled, .. } => format!("{}{}{}", if *checked { "✓ " } else { "" }, label, if *enabled { "" } else { " (off)" }),
            })
            .collect()
    }

    #[test]
    fn the_menu_lists_presets_then_widget_switches_then_the_app() {
        let presets = [preset("night", "Night Sky", true), preset("rock", "Rock & Roll", false)];
        let items = menu_items(&presets, false, false, true);
        assert_eq!(labels(&items), ["✓ Night Sky", "Rock && Roll", "---", "Edit widgets\tWin+Alt+E", "Pause widgets", "---", "Open Lumen", "Quit Lumen"]);
        // Each preset's id maps back to its place in the list.
        assert!(matches!(&items[1], Item::Entry { id, .. } if *id == MENU_PRESET_FIRST + 1));
    }

    #[test]
    fn states_show_as_ticks_and_greyed_lines() {
        let items = menu_items(&[], true, true, false);
        assert_eq!(labels(&items), ["No presets yet (off)", "---", "✓ Edit widgets\tWin+Alt+E (off)", "✓ Pause widgets", "---", "Open Lumen (off)", "Quit Lumen"]);
    }

    #[test]
    fn a_long_list_is_cut_to_the_most_recent() {
        let many: Vec<PresetInfo> = (0..25).map(|n| preset(&format!("p{n}"), &format!("Preset {n}"), false)).collect();
        let shown = menu_items(&many, false, false, true).iter().filter(|i| matches!(i, Item::Entry { id, .. } if *id >= MENU_PRESET_FIRST)).count();
        assert_eq!(shown, MAX_MENU_PRESETS);
    }
}
