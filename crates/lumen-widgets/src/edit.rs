//! What desktop edit mode puts on screen besides the widgets themselves: the
//! dim layer, the selection outline with its handles and size badge, the
//! dashed snap guides, and the hint. Colours come from the theme's paint.

use std::cell::Cell;

use lumen_render::{Canvas, Rect, Weight};
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, GetDC, HDC, ReleaseDC},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
            SetLayeredWindowAttributes, SetWindowPos, ShowWindow, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::w,
};

use crate::paint::Paint;

/// Space around a selected widget that the selection surface covers, in
/// logical pixels: room for the outline, the handles and the size badge.
pub const CHROME_SIDE: f32 = 16.0;
pub const CHROME_TOP: f32 = 40.0;
/// How far the outline sits outside the widget.
const OUTLINE_GAP: f32 = 3.0;
/// Within this distance of a corner, a drag resizes instead of moving.
pub const HANDLE_REACH: f32 = 14.0;
/// How strongly the desktop is dimmed, 0–255.
const DIM_ALPHA: u8 = 120;

pub const HINT_SIZE: (f32, f32) = (520.0, 40.0);
/// A guide line's thickness in logical pixels.
pub const GUIDE_THICKNESS: f32 = 2.0;

thread_local! {
    /// The dim layer's colour (the theme's surface), as a GDI colour.
    static DIM_COLOUR: Cell<u32> = const { Cell::new(0) };
    /// Called when the dim layer is clicked (to clear the selection).
    static ON_DIM_CLICK: Cell<Option<fn()>> = const { Cell::new(None) };
}

/// Creates the layer that dims one monitor. It sits above normal windows and
/// below the widgets, and is not drawn by the GPU: a plain translucent window.
pub fn create_dim(bounds: (i32, i32, i32, i32), paint: &Paint, on_click: fn()) -> Option<HWND> {
    set_dim_colour(paint);
    ON_DIM_CLICK.set(Some(on_click));
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = w!("LumenDim");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(dim_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
        let style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let (x, y, width, height) = bounds;
        let window = CreateWindowExW(style, class, w!("Lumen edit mode"), WS_POPUP, x, y, width, height, None, None, Some(instance.into()), None).ok()?;
        let _ = SetLayeredWindowAttributes(window, COLORREF(0), DIM_ALPHA, LWA_ALPHA);
        let _ = SetWindowPos(window, Some(HWND_TOPMOST), x, y, width, height, SWP_NOACTIVATE);
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        Some(window)
    }
}

pub fn destroy_dim(window: HWND) {
    unsafe {
        let _ = DestroyWindow(window);
    }
}

/// Recolours the dim layers after a theme change.
pub fn set_dim_colour(paint: &Paint) {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
    // GDI colours are 0x00BBGGRR.
    DIM_COLOUR.set(byte(paint.surface.r) | byte(paint.surface.g) << 8 | byte(paint.surface.b) << 16);
}

pub fn repaint_dim(window: HWND) {
    unsafe {
        let dc = GetDC(Some(window));
        fill(window, dc);
        ReleaseDC(Some(window), dc);
    }
}

unsafe fn fill(window: HWND, dc: HDC) {
    unsafe {
        let mut rect = RECT::default();
        let _ = GetClientRect(window, &mut rect);
        let brush = CreateSolidBrush(COLORREF(DIM_COLOUR.get()));
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

unsafe extern "system" fn dim_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_ERASEBKGND => {
            unsafe { fill(window, HDC(wparam.0 as *mut _)) };
            LRESULT(1)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            if let Some(on_click) = ON_DIM_CLICK.get() {
                on_click();
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Draws the selection around a widget of `size`, on a surface that extends
/// [`CHROME_SIDE`] / [`CHROME_TOP`] beyond it.
pub fn draw_selection(canvas: &Canvas<'_>, paint: &Paint, size: (f32, f32), widget_radius: f32) {
    let widget = Rect::new(CHROME_SIDE, CHROME_TOP, size.0, size.1);
    let outline = widget.inset(-OUTLINE_GAP - 2.0);
    canvas.stroke_round_rect(outline, widget_radius + OUTLINE_GAP + 2.0, 2.0, paint.primary);

    // Four corner handles.
    let handle = 10.0;
    for (x, y) in [(outline.x, outline.y), (outline.x + outline.width, outline.y), (outline.x, outline.y + outline.height), (outline.x + outline.width, outline.y + outline.height)] {
        let square = Rect::new(x - handle / 2.0 - 1.0, y - handle / 2.0 - 1.0, handle + 2.0, handle + 2.0);
        canvas.fill_round_rect(square, 3.0, paint.primary);
        canvas.fill_round_rect(square.inset(2.0), 1.5, paint.text);
    }

    // Size badge above the top-right corner.
    let label = format!("{} × {}", size.0.round(), size.1.round());
    let badge_width = 12.0 + label.chars().count() as f32 * 7.2;
    let badge = Rect::new(widget.x + widget.width - badge_width, widget.y - 34.0, badge_width, 22.0);
    canvas.fill_round_rect(badge, 6.0, paint.primary);
    let mut style = paint.body(12.0, Weight::SemiBold, paint.surface);
    style.align_right = true;
    canvas.text(&label, Rect::new(badge.x, badge.y + 3.0, badge.width - 7.0, 18.0), style);
}

/// Draws a dashed line along the surface's long side.
pub fn draw_guide(canvas: &Canvas<'_>, paint: &Paint, size: (f32, f32)) {
    let vertical = size.1 > size.0;
    let length = if vertical { size.1 } else { size.0 };
    let (dash, gap) = (7.0, 5.0);
    let mut at = 0.0;
    while at < length {
        let piece = if vertical { Rect::new(0.0, at, size.0, dash) } else { Rect::new(at, 0.0, dash, size.1) };
        canvas.fill_round_rect(piece, 0.0, paint.primary);
        at += dash + gap;
    }
}

pub fn draw_hint(canvas: &Canvas<'_>, paint: &Paint, size: (f32, f32), locked: bool) {
    let bounds = Rect::new(0.0, 0.0, size.0, size.1);
    canvas.fill_round_rect(bounds, size.1 / 2.0, paint.surface);
    canvas.stroke_round_rect(bounds, size.1 / 2.0, 1.0, paint.border);
    let text = if locked { "Layout is locked · unlock it in Lumen to move widgets · Esc to finish" } else { "Drag to move · drag a corner to resize · Esc to finish" };
    canvas.text(text, Rect::new(20.0, 11.0, size.0 - 40.0, 20.0), paint.body(14.0, Weight::Medium, paint.text));
}
