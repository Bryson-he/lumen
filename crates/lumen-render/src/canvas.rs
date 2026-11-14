//! Drawing calls for one frame of a surface. Coordinates are logical pixels
//! (96 per inch); Direct2D scales them to the monitor's DPI.

use windows::{
    Win32::Graphics::{
        Direct2D::{
            Common::{D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT},
            D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_ROUND, D2D1_ELLIPSE, D2D1_LINE_JOIN_ROUND, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE, ID2D1Factory, ID2D1PathGeometry, ID2D1StrokeStyle,
            D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, D2D1_BRUSH_PROPERTIES, D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED,
            D2D1_COLOR_SPACE_SRGB, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_EXTEND_MODE_CLAMP, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_ROUNDED_RECT, ID2D1Bitmap1,
            ID2D1Brush, ID2D1DeviceContext,
        },
        DirectWrite::{
            DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
            DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
        },
        Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
    },
    core::{HSTRING, w},
};
use windows_numerics::{Matrix3x2, Vector2};

use crate::{Context, Gpu, Result};

/// A colour with opacity. Components are 0.0–1.0, not premultiplied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    /// An opaque colour from 8-bit sRGB components.
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r: f32::from(r) / 255.0, g: f32::from(g) / 255.0, b: f32::from(b) / 255.0, a: 1.0 }
    }

    /// The same colour at a different opacity.
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    fn d2d(self) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r: self.r, g: self.g, b: self.b, a: self.a }
    }
}

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Shrinks the rectangle by `by` on every side.
    pub fn inset(self, by: f32) -> Self {
        Self::new(self.x + by, self.y + by, (self.width - 2.0 * by).max(0.0), (self.height - 2.0 * by).max(0.0))
    }

    fn d2d(self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.x + self.width, bottom: self.y + self.height }
    }

    fn rounded(self, radius: f32) -> D2D1_ROUNDED_RECT {
        // A radius larger than half the shorter side would bulge.
        let radius = radius.min(self.width / 2.0).min(self.height / 2.0).max(0.0);
        D2D1_ROUNDED_RECT { rect: self.d2d(), radiusX: radius, radiusY: radius }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
}

/// How to draw a piece of text.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle<'a> {
    /// Font family name. Bundled fonts are found first, then installed ones;
    /// an unknown family falls back to the system UI font.
    pub family: &'a str,
    /// Font size in logical pixels.
    pub size: f32,
    pub weight: Weight,
    pub color: Rgba,
    /// Align to the right edge of the box instead of the left.
    pub align_right: bool,
    /// Centre in the box (wins over `align_right`).
    pub align_center: bool,
}

/// An image uploaded to the GPU, ready to draw.
#[derive(Clone)]
pub struct Bitmap {
    pub(crate) inner: ID2D1Bitmap1,
    pub width: u32,
    pub height: u32,
}

/// Drawing surface for one frame. Created by [`Surface::draw`](crate::Surface::draw).
pub struct Canvas<'a> {
    pub(crate) dc: &'a ID2D1DeviceContext,
    pub(crate) gpu: &'a Gpu,
}

impl Canvas<'_> {
    pub fn fill_round_rect(&self, rect: Rect, radius: f32, color: Rgba) {
        if let Some(brush) = self.brush(color) {
            unsafe { self.dc.FillRoundedRectangle(&rect.rounded(radius), &brush) };
        }
    }

    /// Draws an outline `width` pixels thick, just inside `rect`.
    pub fn stroke_round_rect(&self, rect: Rect, radius: f32, width: f32, color: Rgba) {
        if width <= 0.0 {
            return;
        }
        if let Some(brush) = self.brush(color) {
            // Strokes are centred on the path; move it in by half so it stays inside.
            let inner = rect.inset(width / 2.0);
            unsafe { self.dc.DrawRoundedRectangle(&inner.rounded(radius - width / 2.0), &brush, width, None) };
        }
    }

    /// Fills a rounded rectangle with a diagonal gradient (top-left to bottom-right).
    pub fn fill_round_rect_gradient(&self, rect: Rect, radius: f32, from: Rgba, to: Rgba) {
        let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: from.d2d() }, D2D1_GRADIENT_STOP { position: 1.0, color: to.d2d() }];
        let properties =
            D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: Vector2 { X: rect.x, Y: rect.y }, endPoint: Vector2 { X: rect.x + rect.width, Y: rect.y + rect.height } };
        unsafe {
            let Ok(collection) = self.dc.CreateGradientStopCollection(
                &stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                D2D1_EXTEND_MODE_CLAMP,
                D2D1_COLOR_INTERPOLATION_MODE_PREMULTIPLIED,
            ) else {
                return;
            };
            let Ok(brush) = self.dc.CreateLinearGradientBrush(&properties, None, &collection) else {
                return;
            };
            self.dc.FillRoundedRectangle(&rect.rounded(radius), &brush);
        }
    }

    /// A new, empty path, and a stroke with rounded ends and corners to draw it with.
    fn path(&self) -> Option<(ID2D1PathGeometry, Option<ID2D1StrokeStyle>)> {
        unsafe {
            let factory: ID2D1Factory = self.dc.GetFactory().ok()?;
            let round = D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, dashCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, miterLimit: 1.0, ..Default::default() };
            Some((factory.CreatePathGeometry().ok()?, factory.CreateStrokeStyle(&round, None).ok()))
        }
    }

    /// Draws a line through `points`, `width` pixels thick, with rounded ends and corners.
    pub fn polyline(&self, points: &[(f32, f32)], width: f32, color: Rgba) {
        let (Some(first), true) = (points.first(), points.len() >= 2 && width > 0.0) else { return };
        let (Some(brush), Some((geometry, stroke))) = (self.brush(color), self.path()) else { return };
        unsafe {
            let Ok(sink) = geometry.Open() else { return };
            sink.BeginFigure(Vector2 { X: first.0, Y: first.1 }, D2D1_FIGURE_BEGIN_HOLLOW);
            let rest: Vec<Vector2> = points[1..].iter().map(|point| Vector2 { X: point.0, Y: point.1 }).collect();
            sink.AddLines(&rest);
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_ok() {
                self.dc.DrawGeometry(&geometry, &brush, width, stroke.as_ref());
            }
        }
    }

    /// Draws part of a circle's outline: from `start` degrees (0 is the top,
    /// going clockwise) round by `sweep` degrees, `width` pixels thick with
    /// rounded ends. A sweep of 360 or more is the whole circle.
    pub fn arc(&self, centre: (f32, f32), radius: f32, start: f32, sweep: f32, width: f32, color: Rgba) {
        if radius <= 0.0 || width <= 0.0 || sweep <= 0.0 {
            return;
        }
        let Some(brush) = self.brush(color) else { return };
        unsafe {
            if sweep >= 359.9 {
                self.dc.DrawEllipse(&D2D1_ELLIPSE { point: Vector2 { X: centre.0, Y: centre.1 }, radiusX: radius, radiusY: radius }, &brush, width, None);
                return;
            }
            let at = |degrees: f32| {
                let angle = degrees.to_radians();
                Vector2 { X: centre.0 + radius * angle.sin(), Y: centre.1 - radius * angle.cos() }
            };
            let Some((geometry, stroke)) = self.path() else { return };
            let Ok(sink) = geometry.Open() else { return };
            sink.BeginFigure(at(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: at(start + sweep),
                size: D2D_SIZE_F { width: radius, height: radius },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_ok() {
                self.dc.DrawGeometry(&geometry, &brush, width, stroke.as_ref());
            }
        }
    }

    /// Draws one line of text in `bounds`, cut off with an ellipsis if it is too long.
    pub fn text(&self, text: &str, bounds: Rect, style: TextStyle<'_>) {
        if text.is_empty() || bounds.width <= 0.0 {
            return;
        }
        let weight = DWRITE_FONT_WEIGHT(match style.weight {
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::SemiBold => 600,
        });
        let (collection, family) = self.gpu.font_for(style.family);
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let format = self.gpu.dwrite.CreateTextFormat(
                &HSTRING::from(family.as_str()),
                collection.as_ref(),
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                w!(""),
            );
            let Ok(format) = format else { return };
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let alignment = if style.align_center {
                DWRITE_TEXT_ALIGNMENT_CENTER
            } else if style.align_right {
                DWRITE_TEXT_ALIGNMENT_TRAILING
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            };
            let _ = format.SetTextAlignment(alignment);
            if let Ok(ellipsis) = self.gpu.dwrite.CreateEllipsisTrimmingSign(&format) {
                let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                let _ = format.SetTrimming(&trimming, &ellipsis);
            }
            let Ok(layout) = self.gpu.dwrite.CreateTextLayout(&wide, &format, bounds.width, bounds.height.max(1.0)) else {
                return;
            };
            if let Some(brush) = self.brush(style.color) {
                self.dc.DrawTextLayout(Vector2 { X: bounds.x, Y: bounds.y }, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
            }
        }
    }

    /// How wide a line of text would be, in logical pixels (0 if it cannot be measured).
    pub fn text_width(&self, text: &str, style: TextStyle<'_>) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let weight = DWRITE_FONT_WEIGHT(match style.weight {
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::SemiBold => 600,
        });
        let (collection, family) = self.gpu.font_for(style.family);
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let format = self.gpu.dwrite.CreateTextFormat(
                &HSTRING::from(family.as_str()),
                collection.as_ref(),
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                w!(""),
            );
            let Ok(format) = format else { return 0.0 };
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let Ok(layout) = self.gpu.dwrite.CreateTextLayout(&wide, &format, 10_000.0, 100.0) else {
                return 0.0;
            };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return 0.0;
            }
            metrics.widthIncludingTrailingWhitespace.ceil()
        }
    }

    /// Uploads 8-bit RGBA pixels (not premultiplied) as a bitmap.
    pub fn bitmap_from_rgba(&self, width: u32, height: u32, rgba: &[u8]) -> Result<Bitmap> {
        assert_eq!(rgba.len(), (width * height * 4) as usize, "pixel buffer does not match its size");
        // Direct2D wants BGRA with colour already multiplied by alpha.
        let mut bgra = Vec::with_capacity(rgba.len());
        for pixel in rgba.chunks_exact(4) {
            let alpha = u32::from(pixel[3]);
            let scale = |c: u8| ((u32::from(c) * alpha + 127) / 255) as u8;
            bgra.extend_from_slice(&[scale(pixel[2]), scale(pixel[1]), scale(pixel[0]), pixel[3]]);
        }
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            ..Default::default()
        };
        let inner = unsafe { self.dc.CreateBitmap(D2D_SIZE_U { width, height }, Some(bgra.as_ptr().cast()), width * 4, &properties) }.context("uploading an image")?;
        Ok(Bitmap { inner, width, height })
    }

    /// Draws a bitmap to fill `rect` (cropping to keep its shape), with rounded corners.
    pub fn image(&self, bitmap: &Bitmap, rect: Rect, radius: f32) {
        if bitmap.width == 0 || bitmap.height == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        // Scale to cover the rectangle, centred.
        let scale = (rect.width / bitmap.width as f32).max(rect.height / bitmap.height as f32);
        let offset_x = rect.x + (rect.width - bitmap.width as f32 * scale) / 2.0;
        let offset_y = rect.y + (rect.height - bitmap.height as f32 * scale) / 2.0;
        let transform = Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: offset_x, M32: offset_y };
        let brush_properties = D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform };
        unsafe {
            let Ok(brush) = self.dc.CreateBitmapBrush(&bitmap.inner, None, Some(&brush_properties)) else {
                return;
            };
            self.dc.FillRoundedRectangle(&rect.rounded(radius), &brush);
        }
    }

    fn brush(&self, color: Rgba) -> Option<ID2D1Brush> {
        use windows::core::Interface;
        unsafe { self.dc.CreateSolidColorBrush(&color.d2d(), None) }.ok()?.cast().ok()
    }
}
