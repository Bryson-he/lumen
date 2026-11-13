//! Direct2D + DirectWrite drawing on DirectComposition windows, for desktop
//! widgets (and later the bar).
//!
//! One [`Gpu`] is shared by every [`Surface`]: one Direct3D device, one Direct2D
//! device, one DirectWrite factory. A surface is a borderless, never-activated
//! window at the bottom of the z-order with per-pixel transparency. Nothing is
//! drawn unless [`Surface::draw`] is called: there is no render loop.
//!
//! All of this lives on one thread (the one that creates the `Gpu`).

mod canvas;
mod gpu;
mod surface;

pub use canvas::{Bitmap, Canvas, Rect, Rgba, TextStyle, Weight};
pub use gpu::Gpu;
pub use surface::{InputHandler, Surface, SurfaceOptions, set_input_handler};

/// Everything that can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{what}: {source}")]
    Windows { what: &'static str, source: windows::core::Error },
    /// The graphics device was reset (driver update, sleep, remote session).
    /// Drop every [`Surface`] and the [`Gpu`], then create them again.
    #[error("the graphics device was lost")]
    DeviceLost,
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) trait Context<T> {
    fn context(self, what: &'static str) -> Result<T>;
}

impl<T> Context<T> for windows::core::Result<T> {
    fn context(self, what: &'static str) -> Result<T> {
        self.map_err(|source| Error::Windows { what, source })
    }
}

/// Makes this process draw at each monitor's own scale instead of being
/// stretched by Windows. Call once, before creating any window.
pub fn enable_per_monitor_dpi() {
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
    unsafe {
        // Fails harmlessly if awareness was already set (for example by a manifest).
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
