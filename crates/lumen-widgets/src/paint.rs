//! What a widget draws with. Every colour here is derived from the theme's
//! roles; widgets never hold colours of their own.

use lumen_render::{Canvas, Rect, Rgba, TextStyle, Weight};
use lumen_theme::{Blur, Color, Theme};

/// The headline font (clock digits), as in the app.
const DISPLAY_FONT: &str = "Space Grotesk";

#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    /// The widget's see-through background.
    pub glass: Rgba,
    pub border: Rgba,
    pub border_width: f32,
    pub radius: f32,
    pub text: Rgba,
    pub text_muted: Rgba,
    /// The empty part of a progress bar.
    pub track: Rgba,
    pub primary: Rgba,
    pub tertiary: Rgba,
    pub accent: Rgba,
    pub body_font: String,
}

impl Paint {
    pub fn from_theme(theme: &Theme) -> Self {
        let roles = &theme.roles;
        // True backdrop blur is not available yet, so the blur setting controls
        // how see-through the glass is: more "blur" lets more wallpaper show.
        let glass_opacity = match theme.style.blur {
            Blur::None => 1.0,
            Blur::Low => 0.9,
            Blur::Medium => 0.78,
            Blur::High => 0.64,
        };
        Self {
            glass: rgba(roles.surface).with_alpha(glass_opacity),
            border: rgba(roles.on_surface).with_alpha(0.14),
            border_width: f32::from(theme.style.border),
            radius: f32::from(theme.style.radius),
            text: rgba(roles.on_surface),
            text_muted: rgba(roles.on_surface).with_alpha(0.72),
            track: rgba(roles.outline).with_alpha(0.55),
            primary: rgba(roles.primary),
            tertiary: rgba(roles.tertiary),
            accent: rgba(roles.accent),
            body_font: theme.style.font.clone(),
        }
    }

    /// The glass card every widget sits on.
    pub fn card(&self, canvas: &Canvas<'_>, width: f32, height: f32) {
        let bounds = Rect::new(0.0, 0.0, width, height);
        canvas.fill_round_rect(bounds, self.radius, self.glass);
        canvas.stroke_round_rect(bounds, self.radius, self.border_width, self.border);
    }

    /// A thin progress bar; `fraction` is 0.0–1.0.
    pub fn bar(&self, canvas: &Canvas<'_>, rect: Rect, fraction: f32, fill: Rgba) {
        canvas.fill_round_rect(rect, rect.height / 2.0, self.track);
        let fraction = fraction.clamp(0.0, 1.0);
        if fraction > 0.0 {
            // Never narrower than its own height, so a small value is still a dot.
            let width = (rect.width * fraction).max(rect.height);
            canvas.fill_round_rect(Rect::new(rect.x, rect.y, width, rect.height), rect.height / 2.0, fill);
        }
    }

    pub fn body(&self, size: f32, weight: Weight, color: Rgba) -> TextStyle<'_> {
        TextStyle { family: &self.body_font, size, weight, color, align_right: false }
    }

    pub fn display(&self, size: f32, color: Rgba) -> TextStyle<'_> {
        TextStyle { family: DISPLAY_FONT, size, weight: Weight::SemiBold, color, align_right: false }
    }
}

fn rgba(color: Color) -> Rgba {
    Rgba::rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use lumen_theme::{Mode, Variant};

    use super::*;

    fn theme() -> Theme {
        Theme::from_color(Color::new(0x2F, 0x5B, 0xEA), Variant::Tonal, Mode::Dark, 0).unwrap()
    }

    #[test]
    fn colours_come_from_the_theme_roles() {
        let theme = theme();
        let paint = Paint::from_theme(&theme);
        assert_eq!(paint.primary, rgba(theme.roles.primary));
        assert_eq!(paint.text, rgba(theme.roles.on_surface));
        assert_eq!(paint.glass.with_alpha(1.0), rgba(theme.roles.surface));
        assert_eq!(paint.radius, 12.0);
        assert_eq!(paint.border_width, 1.0);
        assert_eq!(paint.body_font, "Inter");
    }

    #[test]
    fn a_new_theme_gives_new_paint() {
        let dark = Paint::from_theme(&theme());
        let light = Paint::from_theme(&Theme::from_color(Color::new(0xC0, 0x39, 0x2B), Variant::Vibrant, Mode::Light, 0).unwrap());
        assert_ne!(dark.primary, light.primary);
        assert_ne!(dark.glass, light.glass);
        assert_ne!(dark.text, light.text);
    }

    #[test]
    fn blur_setting_changes_how_see_through_the_glass_is() {
        let mut theme = theme();
        let mut opacity = |blur| {
            theme.style.blur = blur;
            Paint::from_theme(&theme).glass.a
        };
        assert_eq!(opacity(Blur::None), 1.0);
        assert!(opacity(Blur::Low) > opacity(Blur::Medium) && opacity(Blur::Medium) > opacity(Blur::High));
    }
}
