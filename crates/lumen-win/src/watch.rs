//! Tells a window when files in a folder change, without polling: a small
//! thread sleeps in Windows until the folder reports a change.

use std::{path::Path, thread};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM},
        Storage::FileSystem::{FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification},
        System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::HSTRING,
};

/// Watches a folder for as long as it is kept; dropping it stops the thread.
pub struct FolderWatch {
    stop: isize,
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        unsafe {
            // The thread closes both handles when it sees this.
            let _ = SetEvent(HANDLE(self.stop as *mut _));
        }
    }
}

/// Posts `message` to `window` whenever a file in `folder` is created,
/// renamed, deleted or written (not in its sub-folders). Several messages may
/// arrive for one save. `None` if the folder cannot be watched.
pub fn watch_folder(folder: &Path, window: isize, message: u32) -> Option<FolderWatch> {
    unsafe {
        let changes = FindFirstChangeNotificationW(&HSTRING::from(folder.as_os_str()), false, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE).ok()?;
        let Ok(stop) = CreateEventW(None, true, false, None) else {
            let _ = FindCloseChangeNotification(changes);
            return None;
        };
        // Handles are plain numbers to Windows; as numbers they can cross to the thread.
        let (changes_raw, stop_raw) = (changes.0 as isize, stop.0 as isize);
        let spawned = thread::Builder::new().name("folder-watch".into()).spawn(move || {
            let (changes, stop) = (HANDLE(changes_raw as *mut _), HANDLE(stop_raw as *mut _));
            loop {
                // Asleep until the folder changes or the watch is dropped.
                let woke = WaitForMultipleObjects(&[stop, changes], false, INFINITE);
                if woke.0 != WAIT_OBJECT_0.0 + 1 {
                    break;
                }
                let _ = PostMessageW(Some(HWND(window as *mut _)), message, WPARAM(0), LPARAM(0));
                if FindNextChangeNotification(changes).is_err() {
                    break;
                }
            }
            let _ = FindCloseChangeNotification(changes);
            let _ = CloseHandle(stop);
        });
        if spawned.is_err() {
            let _ = FindCloseChangeNotification(changes);
            let _ = CloseHandle(stop);
            return None;
        }
        Some(FolderWatch { stop: stop_raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_folder_cannot_be_watched() {
        assert!(watch_folder(Path::new(r"C:\this\folder\is\not\here"), 0, 0).is_none());
    }

    #[test]
    fn watching_and_stopping_a_real_folder_works() {
        let folder = std::env::temp_dir();
        let watch = watch_folder(&folder, 0, 0);
        assert!(watch.is_some());
        drop(watch);
    }
}
