//! What a widget draws with. Every colour here is derived from the theme's
//! roles; widgets never hold colours of their own.

use lumen_ipc::{
    style::{ComponentStyle, Role, StyleBackground},
    widgets::{Background, WidgetStyle},
};
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
    /// The theme's surface colour, fully opaque (edit-mode chrome, solid backgrounds).
    pub surface: Rgba,
    pub body_font: String,
    /// Where the background fades to, for a gradient style.
    pub glass_to: Option<Rgba>,
    /// Multiplies every text size (a component style's "text size").
    pub text_scale: f32,
    /// Multiplies padding and gaps where a piece lays itself out (bar, flyout, launcher).
    pub spacing: f32,
    /// The theme this paint came from, for resolving a style's roles.
    theme: ThemeBase,
}

/// What a component style needs of the theme: its role colours and its own style values.
#[derive(Debug, Clone, PartialEq)]
struct ThemeBase {
    roles: [Rgba; 8],
    glass_opacity: f32,
    radius: f32,
    border_width: f32,
    font: String,
}

impl ThemeBase {
    fn role(&self, role: Role) -> Rgba {
        self.roles[match role {
            Role::Primary => 0,
            Role::Secondary => 1,
            Role::Tertiary => 2,
            Role::Neutral => 3,
            Role::Surface => 4,
            Role::OnSurface => 5,
            Role::Outline => 6,
            Role::Accent => 7,
        }]
    }
}

impl Paint {
    /// This paint with one widget's own style choices applied.
    pub fn for_widget(&self, style: &WidgetStyle) -> Self {
        let mut paint = self.clone();
        match style.background {
            Background::Glass => {}
            Background::Solid => {
                paint.glass = paint.glass.with_alpha(1.0);
                paint.glass_to = paint.glass_to.map(|to| to.with_alpha(1.0));
            }
            Background::None => {
                paint.glass = Rgba::TRANSPARENT;
                paint.glass_to = None;
                paint.border_width = 0.0;
            }
        }
        if let Some(radius) = style.radius {
            paint.radius = f32::from(radius);
        }
        paint
    }

    /// This paint with a component style applied: its shape, and its
    /// colours looked up among the theme's roles. `over_windows` is for
    /// pieces that appear on top of other windows (flyouts, the launcher, a
    /// floating bar): glass would be unreadable there, so it becomes solid.
    pub fn styled(&self, style: &ComponentStyle, over_windows: bool) -> Self {
        let theme = &self.theme;
        let background = theme.role(style.background_role);
        let opacity = f32::from(style.opacity) / 100.0;
        let (glass, glass_to) = match style.background {
            StyleBackground::Glass => (background.with_alpha(if over_windows { 1.0 } else { theme.glass_opacity }), None),
            StyleBackground::Solid => (background.with_alpha(opacity), None),
            StyleBackground::Gradient => (background.with_alpha(opacity), Some(theme.role(style.gradient_role).with_alpha(opacity))),
            StyleBackground::None => (Rgba::TRANSPARENT, None),
        };
        let text = theme.role(style.text_role);
        // The default border is a faint line of the text colour; any other role is used at full strength.
        let border = if style.border_role == Role::OnSurface { theme.role(Role::OnSurface).with_alpha(0.14) } else { theme.role(style.border_role) };
        Self {
            glass,
            glass_to,
            border,
            border_width: if style.border_follows_theme { theme.border_width } else { f32::from(style.border_width) },
            radius: if style.corners_follow_theme { theme.radius } else { f32::from(style.radius) },
            text,
            text_muted: text.with_alpha(0.72),
            track: self.track,
            primary: theme.role(style.accent_role),
            tertiary: self.tertiary,
            accent: self.accent,
            surface: self.surface,
            body_font: style.font.clone().unwrap_or_else(|| theme.font.clone()),
            text_scale: f32::from(style.text_scale) / 100.0,
            spacing: f32::from(style.spacing) / 100.0,
            theme: theme.clone(),
        }
    }

    /// A length of padding or gap, scaled by the style's spacing.
    pub fn space(&self, length: f32) -> f32 {
        length * self.spacing
    }

    /// Fills a piece's background: the glass colour, or the style's gradient.
    pub fn fill_background(&self, canvas: &Canvas<'_>, rect: Rect, radius: f32) {
        match self.glass_to {
            Some(to) => canvas.fill_round_rect_gradient(rect, radius, self.glass, to),
            None => canvas.fill_round_rect(rect, radius, self.glass),
        }
    }

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
            surface: rgba(roles.surface),
            body_font: theme.style.font.clone(),
            glass_to: None,
            text_scale: 1.0,
            spacing: 1.0,
            theme: ThemeBase {
                roles: [roles.primary, roles.secondary, roles.tertiary, roles.neutral, roles.surface, roles.on_surface, roles.outline, roles.accent].map(rgba),
                glass_opacity,
                radius: f32::from(theme.style.radius),
                border_width: f32::from(theme.style.border),
                font: theme.style.font.clone(),
            },
        }
    }

    /// The glass card every widget sits on.
    pub fn card(&self, canvas: &Canvas<'_>, width: f32, height: f32) {
        let bounds = Rect::new(0.0, 0.0, width, height);
        // A radius past half the shorter side makes a pill; no further.
        let radius = self.radius.min(width.min(height) / 2.0);
        self.fill_background(canvas, bounds, radius);
        if self.border_width > 0.0 {
            canvas.stroke_round_rect(bounds, radius, self.border_width, self.border);
        }
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
        TextStyle { family: &self.body_font, size: size * self.text_scale, weight, color, align_right: false }
    }

    pub fn display(&self, size: f32, color: Rgba) -> TextStyle<'_> {
        TextStyle { family: DISPLAY_FONT, size: size * self.text_scale, weight: Weight::SemiBold, color, align_right: false }
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
    fn the_default_style_changes_nothing() {
        let base = Paint::from_theme(&theme());
        assert_eq!(base.styled(&ComponentStyle::default(), false), base);
        let over = base.styled(&ComponentStyle::default(), true);
        assert_eq!(over.glass, base.surface, "glass turns solid over other windows");
    }

    #[test]
    fn a_style_picks_roles_and_shape_and_follows_the_theme() {
        let style = ComponentStyle {
            background: StyleBackground::Gradient,
            background_role: Role::Neutral,
            gradient_role: Role::Primary,
            opacity: 50,
            corners_follow_theme: false,
            radius: 30,
            border_follows_theme: false,
            border_width: 3,
            border_role: Role::Accent,
            text_role: Role::Primary,
            accent_role: Role::Tertiary,
            font: Some("Space Grotesk".into()),
            text_scale: 120,
            spacing: 80,
            ..Default::default()
        };
        for theme in [theme(), Theme::from_color(Color::new(0xC0, 0x39, 0x2B), Variant::Vibrant, Mode::Light, 0).unwrap()] {
            let paint = Paint::from_theme(&theme).styled(&style, false);
            assert_eq!(paint.glass, rgba(theme.roles.neutral).with_alpha(0.5));
            assert_eq!(paint.glass_to, Some(rgba(theme.roles.primary).with_alpha(0.5)));
            assert_eq!(paint.border, rgba(theme.roles.accent));
            assert_eq!(paint.text, rgba(theme.roles.primary));
            assert_eq!(paint.primary, rgba(theme.roles.tertiary), "highlights use the accent role");
            assert_eq!((paint.radius, paint.border_width, paint.body_font.as_str()), (30.0, 3.0, "Space Grotesk"));
            assert_eq!(paint.body(10.0, Weight::Regular, paint.text).size, 12.0);
            assert_eq!(paint.space(20.0), 16.0);
        }
    }

    #[test]
    fn a_widgets_own_choices_apply_on_top_of_its_style() {
        let styled = Paint::from_theme(&theme()).styled(&ComponentStyle { background: StyleBackground::Gradient, opacity: 60, ..Default::default() }, false);
        let solid = styled.for_widget(&WidgetStyle { background: Background::Solid, ..Default::default() });
        assert_eq!((solid.glass.a, solid.glass_to.map(|to| to.a)), (1.0, Some(1.0)), "solid keeps the style's colours, fully opaque");
        let bare = styled.for_widget(&WidgetStyle { background: Background::None, ..Default::default() });
        assert_eq!((bare.glass.a, bare.glass_to), (0.0, None));
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
    fn a_widgets_own_style_overrides_the_theme() {
        let base = Paint::from_theme(&theme());
        let glass = base.for_widget(&WidgetStyle::default());
        assert_eq!(glass, base, "the default style changes nothing");

        let solid = base.for_widget(&WidgetStyle { background: Background::Solid, radius: Some(4), ..Default::default() });
        assert_eq!(solid.glass, base.surface);
        assert_eq!(solid.glass.a, 1.0);
        assert_eq!(solid.radius, 4.0);

        let bare = base.for_widget(&WidgetStyle { background: Background::None, ..Default::default() });
        assert_eq!(bare.glass.a, 0.0);
        assert_eq!(bare.border_width, 0.0);
        assert_eq!(bare.radius, base.radius, "corners still follow the theme");
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
