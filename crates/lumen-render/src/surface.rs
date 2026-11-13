//! A widget window: borderless, transparent, never activated, drawn with
//! Direct2D through DirectComposition. Normally it sits at the bottom of the
//! z-order and lets clicks through; edit mode raises it and makes it draggable.

use std::{cell::Cell, rc::Rc, sync::Once};

use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
                D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, ID2D1DeviceContext,
            },
            DirectComposition::{IDCompositionTarget, IDCompositionVisual},
            Dxgi::{
                Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC},
                DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGISurface, IDXGISwapChain1,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetWindowLongW, HWND_BOTTOM, HWND_NOTOPMOST, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE,
                RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongW,
                SetWindowPos, ShowWindow, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::w,
};

use crate::{Canvas, Context, Error, Gpu, Result};

/// The Direct2D error for "the target must be recreated".
const D2DERR_RECREATE_TARGET: windows::core::HRESULT = windows::core::HRESULT(0x8899000C_u32 as i32);

/// Receives mouse messages for surfaces that are not click-through:
/// `(window, message, wparam, lparam)`. Return `Some(result)` to handle one.
pub type InputHandler = fn(isize, u32, usize, isize) -> Option<isize>;

thread_local! {
    static INPUT: Cell<Option<InputHandler>> = const { Cell::new(None) };
}

/// Sets (or clears) this thread's handler for mouse input on its surfaces.
pub fn set_input_handler(handler: Option<InputHandler>) {
    INPUT.set(handler);
}

/// Where and how to create a surface.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceOptions {
    /// Top-left corner on the virtual desktop, in physical pixels.
    pub x: i32,
    pub y: i32,
    /// Size in logical pixels (96 per inch); scaled to the monitor's DPI.
    pub width: f32,
    pub height: f32,
    /// Let mouse clicks fall through to whatever is underneath.
    pub click_through: bool,
    /// Keep above other windows instead of at the bottom.
    pub topmost: bool,
}

pub struct Surface {
    gpu: Rc<Gpu>,
    window: HWND,
    swap_chain: IDXGISwapChain1,
    dc: ID2D1DeviceContext,
    // Kept alive: they connect the swap chain to the window.
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
    options: SurfaceOptions,
    scale: f32,
    visible: bool,
}

impl Surface {
    pub fn new(gpu: &Rc<Gpu>, options: SurfaceOptions) -> Result<Self> {
        static CLASS: Once = Once::new();
        let class = w!("LumenWidget");
        unsafe {
            let instance = GetModuleHandleW(None).context("getting the module handle")?;
            CLASS.call_once(|| {
                RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() });
            });

            // NOREDIRECTIONBITMAP: the window has no GDI surface, only our composition visual.
            // TOOLWINDOW keeps it out of Alt+Tab and the taskbar; NOACTIVATE means it never takes focus.
            // LAYERED is always on so that click-through (TRANSPARENT) can be switched later.
            let mut style = WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED;
            if options.click_through {
                style |= WS_EX_TRANSPARENT;
            }
            // Created at 1x1 first so its monitor's DPI can be read before sizing.
            let window = CreateWindowExW(style, class, w!("Lumen widget"), WS_POPUP, options.x, options.y, 1, 1, None, None, Some(instance.into()), None)
                .context("creating a widget window")?;
            let _ = SetLayeredWindowAttributes(window, COLORREF(0), 255, LWA_ALPHA);
            let scale = GetDpiForWindow(window).max(96) as f32 / 96.0;
            let (width, height) = (physical(options.width, scale), physical(options.height, scale));

            let built = build(gpu, window, width, height, scale);
            let (swap_chain, dc, target, visual) = match built {
                Ok(parts) => parts,
                Err(error) => {
                    let _ = DestroyWindow(window);
                    return Err(error);
                }
            };

            let order = if options.topmost { HWND_TOPMOST } else { HWND_BOTTOM };
            let _ = SetWindowPos(window, Some(order), options.x, options.y, width as i32, height as i32, SWP_NOACTIVATE);
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            Ok(Self { gpu: gpu.clone(), window, swap_chain, dc, _target: target, visual, options, scale, visible: true })
        }
    }

    /// The window handle, as a number (for matching input messages).
    pub fn window(&self) -> isize {
        self.window.0 as isize
    }

    /// Size in logical pixels.
    pub fn size(&self) -> (f32, f32) {
        (self.options.width, self.options.height)
    }

    /// Top-left corner in physical pixels.
    pub fn position(&self) -> (i32, i32) {
        (self.options.x, self.options.y)
    }

    /// The monitor scale this surface was created for (1.0 = 100%).
    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Bytes of pixel memory this surface holds (its two frame buffers).
    pub fn memory_bytes(&self) -> u64 {
        let (width, height) = (physical(self.options.width, self.scale), physical(self.options.height, self.scale));
        u64::from(width) * u64::from(height) * 4 * 2
    }

    /// Draws one frame and shows it. The frame starts fully transparent.
    pub fn draw(&self, paint: impl FnOnce(&Canvas<'_>)) -> Result<()> {
        unsafe {
            self.dc.BeginDraw();
            self.dc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            paint(&Canvas { dc: &self.dc, gpu: &self.gpu });
            let drawn = self.dc.EndDraw(None, None);
            let presented = self.swap_chain.Present(1, DXGI_PRESENT(0)).ok();
            for result in [drawn, presented] {
                result.map_err(device_error("drawing a widget"))?;
            }
            Ok(())
        }
    }

    /// Moves the window (physical pixels).
    pub fn set_position(&mut self, x: i32, y: i32) {
        if (x, y) != (self.options.x, self.options.y) {
            (self.options.x, self.options.y) = (x, y);
            unsafe {
                let _ = SetWindowPos(self.window, None, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
            }
        }
    }

    /// Changes the size (logical pixels). The caller must draw again afterwards.
    pub fn resize(&mut self, width: f32, height: f32) -> Result<()> {
        if (width, height) == (self.options.width, self.options.height) {
            return Ok(());
        }
        let (w, h) = (physical(width, self.scale), physical(height, self.scale));
        unsafe {
            // The context must let go of the old back buffer before it can be resized.
            self.dc.SetTarget(None);
            self.swap_chain.ResizeBuffers(0, w, h, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0)).map_err(device_error("resizing a widget"))?;
            bind_back_buffer(&self.dc, &self.swap_chain, self.scale)?;
            let _ = SetWindowPos(self.window, None, 0, 0, w as i32, h as i32, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER);
        }
        (self.options.width, self.options.height) = (width, height);
        Ok(())
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.visible {
            self.visible = visible;
            unsafe {
                let _ = ShowWindow(self.window, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
            if visible {
                self.apply_order();
            }
        }
    }

    /// Whether the window takes mouse input (`true`) or lets clicks through.
    pub fn set_interactive(&mut self, interactive: bool) {
        self.options.click_through = !interactive;
        unsafe {
            let style = GetWindowLongW(self.window, GWL_EXSTYLE) as u32;
            let wanted = if interactive { style & !WS_EX_TRANSPARENT.0 } else { style | WS_EX_TRANSPARENT.0 };
            if wanted != style {
                SetWindowLongW(self.window, GWL_EXSTYLE, wanted as i32);
            }
        }
    }

    /// Above every normal window (`true`) or at the bottom, on the desktop.
    pub fn set_topmost(&mut self, topmost: bool) {
        self.options.topmost = topmost;
        self.apply_order();
    }

    /// Whole-surface opacity, 0.0–1.0.
    pub fn set_opacity(&self, opacity: f32) {
        unsafe {
            if let Ok(effect) = self.gpu.composition.CreateEffectGroup() {
                let _ = effect.SetOpacity2(opacity.clamp(0.0, 1.0));
                let _ = self.visual.SetEffect(&effect);
                let _ = self.gpu.composition.Commit();
            }
        }
    }

    fn apply_order(&self) {
        unsafe {
            let flags = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE;
            if self.options.topmost {
                let _ = SetWindowPos(self.window, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
            } else {
                let _ = SetWindowPos(self.window, Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags);
                let _ = SetWindowPos(self.window, Some(HWND_BOTTOM), 0, 0, 0, 0, flags);
            }
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
        }
    }
}

fn physical(logical: f32, scale: f32) -> u32 {
    (logical * scale).round().max(1.0) as u32
}

fn device_error(what: &'static str) -> impl Fn(windows::core::Error) -> Error {
    move |error| {
        let code = error.code();
        if code == D2DERR_RECREATE_TARGET || code == DXGI_ERROR_DEVICE_REMOVED || code == DXGI_ERROR_DEVICE_RESET { Error::DeviceLost } else { Error::Windows { what, source: error } }
    }
}

type Parts = (IDXGISwapChain1, ID2D1DeviceContext, IDCompositionTarget, IDCompositionVisual);

/// Creates the swap chain, points a Direct2D context at it, and attaches it to the window.
unsafe fn build(gpu: &Gpu, window: HWND, width: u32, height: u32, scale: f32) -> Result<Parts> {
    unsafe {
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            // Per-pixel transparency: the desktop shows through.
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        let swap_chain = gpu.dxgi_factory.CreateSwapChainForComposition(&gpu.d3d, &description, None).context("creating a swap chain")?;
        let dc = gpu.d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE).context("creating a drawing context")?;
        bind_back_buffer(&dc, &swap_chain, scale)?;

        let target = gpu.composition.CreateTargetForHwnd(window, true).context("attaching to the window")?;
        let visual = gpu.composition.CreateVisual().context("creating a visual")?;
        visual.SetContent(&swap_chain).context("setting the visual's content")?;
        target.SetRoot(&visual).context("setting the root visual")?;
        gpu.composition.Commit().context("committing the composition")?;
        Ok((swap_chain, dc, target, visual))
    }
}

/// Makes the swap chain's back buffer the context's drawing target.
unsafe fn bind_back_buffer(dc: &ID2D1DeviceContext, swap_chain: &IDXGISwapChain1, scale: f32) -> Result<()> {
    unsafe {
        let back_buffer: IDXGISurface = swap_chain.GetBuffer(0).context("getting the back buffer")?;
        let dpi = 96.0 * scale;
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: dpi,
            dpiY: dpi,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            ..Default::default()
        };
        let target_bitmap = dc.CreateBitmapFromDxgiSurface(&back_buffer, Some(&properties)).context("wrapping the back buffer")?;
        dc.SetTarget(&target_bitmap);
        dc.SetDpi(dpi, dpi);
        Ok(())
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Belt and braces with WS_EX_NOACTIVATE: a click never activates a widget.
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if matches!(message, WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE | WM_SETCURSOR | WM_CAPTURECHANGED)
        && let Some(handler) = INPUT.get()
        && let Some(result) = handler(window.0 as isize, message, wparam.0, lparam.0)
    {
        return LRESULT(result);
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
