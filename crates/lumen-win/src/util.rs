use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE},
    },
    core::w,
};
use windows_registry::{CURRENT_USER, Key, Value};

use crate::{Error, Result};

/// Opens (creating if needed) a key under HKEY_CURRENT_USER.
pub(crate) fn hkcu(path: &'static str) -> Result<Key> {
    CURRENT_USER.create(path).map_err(|e| Error::Registry { what: path, message: e.to_string() })
}

/// Reads a DWORD, or `None` if the value does not exist.
pub(crate) fn read_u32(key: &Key, name: &str) -> Option<u32> {
    key.get_u32(name).ok()
}

pub(crate) fn write_u32(key: &Key, name: &'static str, value: u32) -> Result<()> {
    key.set_u32(name, value).map_err(|e| Error::Registry { what: name, message: e.to_string() })
}

/// Reads a value of any type, or `None` if it does not exist.
pub(crate) fn read_value(key: &Key, name: &str) -> Option<Value> {
    key.get_value(name).ok()
}

/// Puts a value back exactly as it was read; `None` deletes it.
pub(crate) fn restore_value(key: &Key, name: &'static str, value: Option<&Value>) -> Result<()> {
    let result = match value {
        Some(value) => key.set_value(name, value),
        None => match key.get_type(name) {
            Ok(_) => key.remove_value(name),
            Err(_) => Ok(()), // already absent
        },
    };
    result.map_err(|e| Error::Registry { what: name, message: e.to_string() })
}

/// Tells running apps that the colour settings changed, the same broadcast
/// Windows Settings sends.
///
/// The broadcast waits for every open window to finish re-theming, which takes
/// seconds on a busy desktop, so it runs on a background thread and this
/// returns at once. Requests made while one is in flight are merged into a
/// single follow-up broadcast. Call [`flush_broadcasts`] before the process exits.
pub(crate) fn broadcast_color_change() {
    let mut state = BROADCAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.running {
        state.again = true;
        return;
    }
    state.running = true;
    state.worker = std::thread::Builder::new()
        .name("color-broadcast".into())
        .spawn(|| {
            loop {
                send_color_change();
                let mut state = BROADCAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if !std::mem::take(&mut state.again) {
                    state.running = false;
                    break;
                }
            }
        })
        .ok();
    if state.worker.is_none() {
        // Could not start a thread: fall back to doing it here.
        state.running = false;
        drop(state);
        send_color_change();
    }
}

/// Waits for any colour-change broadcast still being delivered. Call before
/// exiting, or apps that have not been told yet keep the old colours.
pub fn flush_broadcasts() {
    let worker = BROADCAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).worker.take();
    if let Some(worker) = worker {
        let _ = worker.join();
    }
}

struct Broadcast {
    running: bool,
    /// Another change arrived while broadcasting; send once more when done.
    again: bool,
    worker: Option<std::thread::JoinHandle<()>>,
}

static BROADCAST: std::sync::Mutex<Broadcast> = std::sync::Mutex::new(Broadcast { running: false, again: false, worker: None });

/// Hung windows are skipped rather than waited on.
fn send_color_change() {
    let area = w!("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(area.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
}
