//! Windows API wrappers: wallpaper, light/dark mode, accent colour, fullscreen detection.
//!
//! Everything here uses public Windows APIs and the current user's registry.
//! Nothing injects into, hooks inside, or opens a handle to another process:
//! the fullscreen watcher uses out-of-context WinEvent hooks, which run in
//! Lumen's own process.

pub mod accent;
pub mod appbar;
pub mod audio;
pub mod appearance;
pub mod fullscreen;
pub mod shell;
pub mod wallpaper;

mod util;

pub use util::flush_broadcasts;

/// Everything that can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{what}: {source}")]
    Windows { what: &'static str, source: windows::core::Error },
    #[error("registry: {what}: {message}")]
    Registry { what: &'static str, message: String },
    #[error("wallpaper file not found: {0}")]
    WallpaperNotFound(String),
    #[error("there is no monitor {0}; this PC has {1}")]
    NoSuchMonitor(usize, usize),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Adds a short description to a Windows API error.
pub(crate) trait Context<T> {
    fn context(self, what: &'static str) -> Result<T>;
}

impl<T> Context<T> for windows::core::Result<T> {
    fn context(self, what: &'static str) -> Result<T> {
        self.map_err(|source| Error::Windows { what, source })
    }
}

/// Hands this process's idle memory back to Windows (it is paged in again on
/// demand). Called when a game starts, so Lumen's footprint is as small as it
/// can be while the game runs.
pub fn trim_memory() {
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    unsafe {
        // The pair (usize::MAX, usize::MAX) means "trim the working set now".
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}
