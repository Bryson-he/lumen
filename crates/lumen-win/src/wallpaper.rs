//! Desktop wallpaper, per monitor, through the `IDesktopWallpaper` COM interface.

use std::path::{Path, PathBuf};

use windows::{
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree},
        UI::Shell::{DesktopWallpaper, IDesktopWallpaper},
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

use crate::{Context, Error, Result};

/// One connected monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// 1-based, ordered left to right then top to bottom. Stable while the
    /// monitor arrangement stays the same.
    pub number: usize,
    /// Windows' device path for the monitor; survives reboots.
    pub id: String,
    /// Position and size on the virtual desktop, in pixels.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// The monitor whose top-left corner is the desktop origin.
    pub primary: bool,
    /// The image currently shown, if it is a picture (not a solid colour).
    pub wallpaper: Option<PathBuf>,
}

/// Handle to the desktop wallpaper service. Create one per thread that needs it.
pub struct Wallpapers {
    api: IDesktopWallpaper,
}

impl Wallpapers {
    pub fn new() -> Result<Self> {
        unsafe {
            // Fine if COM is already initialised on this thread, in either mode.
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() && hr != RPC_E_CHANGED_MODE {
                hr.ok().context("starting COM")?;
            }
            let api = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_LOCAL_SERVER).context("opening the desktop wallpaper service")?;
            Ok(Self { api })
        }
    }

    /// Lists connected monitors with their current wallpaper.
    pub fn monitors(&self) -> Result<Vec<Monitor>> {
        let count = unsafe { self.api.GetMonitorDevicePathCount() }.context("counting monitors")?;
        let mut monitors = Vec::new();
        for index in 0..count {
            let id = take_string(unsafe { self.api.GetMonitorDevicePathAt(index) }.context("reading a monitor id")?);
            // Windows also lists monitors that are known but not connected; those have no rectangle.
            let Ok(rect) = (unsafe { self.api.GetMonitorRECT(&HSTRING::from(&id)) }) else { continue };
            monitors.push(Monitor {
                number: 0,
                wallpaper: self.wallpaper_of(&id),
                id,
                x: rect.left,
                y: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
                primary: rect.left == 0 && rect.top == 0,
            });
        }
        monitors.sort_by_key(|m| (m.x, m.y));
        for (index, monitor) in monitors.iter_mut().enumerate() {
            monitor.number = index + 1;
        }
        Ok(monitors)
    }

    /// Shows an image on one monitor (by 1-based number), or on all of them with `None`.
    pub fn set(&self, monitor: Option<usize>, image: impl AsRef<Path>) -> Result<()> {
        let image = image.as_ref();
        if !image.is_file() {
            return Err(Error::WallpaperNotFound(image.display().to_string()));
        }
        // Windows needs a full path, without the \\?\ prefix canonicalize() would add.
        let image = std::path::absolute(image).map_err(|_| Error::WallpaperNotFound(image.display().to_string()))?;
        let image = HSTRING::from(image.as_os_str());

        match monitor {
            None => unsafe { self.api.SetWallpaper(PCWSTR::null(), &image) },
            Some(number) => {
                let monitors = self.monitors()?;
                let target = monitors.iter().find(|m| m.number == number).ok_or(Error::NoSuchMonitor(number, monitors.len()))?;
                unsafe { self.api.SetWallpaper(&HSTRING::from(&target.id), &image) }
            }
        }
        .context("setting the wallpaper")
    }

    fn wallpaper_of(&self, monitor_id: &str) -> Option<PathBuf> {
        let path = take_string(unsafe { self.api.GetWallpaper(&HSTRING::from(monitor_id)) }.ok()?);
        (!path.is_empty()).then(|| PathBuf::from(path))
    }
}

/// Copies a COM-allocated string and frees the original.
fn take_string(raw: PWSTR) -> String {
    unsafe {
        let text = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0.cast()));
        text
    }
}
