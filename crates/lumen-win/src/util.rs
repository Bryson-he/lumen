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
/// Windows Settings sends. Hung windows are skipped rather than waited on.
pub(crate) fn broadcast_color_change() {
    let area = w!("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(area.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
}
