//! Reserving a strip of the screen for a bar, with Windows' public "app bar"
//! API (the same one the taskbar and docked toolbars use). Once reserved,
//! maximised windows stop at the bar instead of sliding underneath it.
//!
//! Nothing here touches the taskbar or any other process: Lumen asks Windows
//! for space for its own window.

use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::Shell::{ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA, SHAppBarMessage},
};

/// Sent in `wParam` of the callback message when the bar should re-reserve
/// its place (another bar or the taskbar moved, or the work area changed).
pub const POSITION_CHANGED: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
}

/// A registered app bar. Dropping it gives the reserved space back.
pub struct AppBar {
    window: HWND,
}

impl AppBar {
    /// Registers `window` as an app bar. Windows posts `callback_message` to
    /// it when something affecting bars changes (see [`POSITION_CHANGED`]).
    pub fn register(window: isize, callback_message: u32) -> Option<Self> {
        let window = HWND(window as *mut _);
        let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: window, uCallbackMessage: callback_message, ..Default::default() };
        (unsafe { SHAppBarMessage(ABM_NEW, &mut data) } != 0).then_some(Self { window })
    }

    /// Reserves a strip `thickness` pixels deep along an edge of a monitor
    /// (`bounds` = x, y, width, height in physical pixels) and returns the
    /// rectangle Windows granted, as (x, y, width, height). If the taskbar or
    /// another bar already holds that edge, the strip is placed beside it.
    pub fn reserve(&self, edge: Edge, bounds: (i32, i32, i32, i32), thickness: i32) -> (i32, i32, i32, i32) {
        let (x, y, width, height) = bounds;
        let mut rect = RECT { left: x, right: x + width, top: y, bottom: y + height };
        match edge {
            Edge::Top => rect.bottom = rect.top + thickness,
            Edge::Bottom => rect.top = rect.bottom - thickness,
        }
        let mut data = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: self.window,
            uEdge: match edge {
                Edge::Top => ABE_TOP,
                Edge::Bottom => ABE_BOTTOM,
            },
            rc: rect,
            ..Default::default()
        };
        unsafe {
            // Ask where the strip may go; Windows moves it clear of other bars...
            SHAppBarMessage(ABM_QUERYPOS, &mut data);
            // ...and may have changed its depth, so set that again before claiming it.
            match edge {
                Edge::Top => data.rc.bottom = data.rc.top + thickness,
                Edge::Bottom => data.rc.top = data.rc.bottom - thickness,
            }
            SHAppBarMessage(ABM_SETPOS, &mut data);
        }
        (data.rc.left, data.rc.top, data.rc.right - data.rc.left, data.rc.bottom - data.rc.top)
    }
}

impl Drop for AppBar {
    fn drop(&mut self) {
        let mut data = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: self.window, ..Default::default() };
        unsafe {
            SHAppBarMessage(ABM_REMOVE, &mut data);
        }
    }
}
