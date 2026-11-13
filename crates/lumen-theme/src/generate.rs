use std::path::Path;

use material_colors::{
    dynamic_color::{DynamicScheme, Variant as MaterialVariant},
    hct::Hct,
    palette::TonalPalette,
};

use crate::{
    Color, Error, Mode, Roles, Source, SourceColors, Style, Theme, Variant, extract,
    theme::name_from_path,
};

/// A second wallpaper colour must be at least this far (in hue degrees) from
/// the seed to be used as the accent; closer than that it reads as the same colour.
const MIN_ACCENT_HUE_DISTANCE: f64 = 45.0;

impl Theme {
    /// Builds a theme from a wallpaper file: [`extract`] then [`Theme::from_wallpaper_colors`].
    pub fn from_wallpaper(path: impl AsRef<Path>, variant: Variant, mode: Mode, contrast: i8) -> Result<Self, Error> {
        let colors = extract(path.as_ref())?;
        Self::from_wallpaper_colors(path, &colors, variant, mode, contrast)
    }

    /// Builds a theme for a wallpaper whose colours were extracted earlier.
    /// The theme is named after the file.
    pub fn from_wallpaper_colors(
        path: impl AsRef<Path>,
        colors: &SourceColors,
        variant: Variant,
        mode: Mode,
        contrast: i8,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let source = Source::Wallpaper { path: path.to_string_lossy().replace('\\', "/") };
        Self::generate(name_from_path(path), source, colors, variant, mode, contrast)
    }

    /// Builds a theme from one picked colour.
    pub fn from_color(color: Color, variant: Variant, mode: Mode, contrast: i8) -> Result<Self, Error> {
        let colors = SourceColors::from_color(color);
        Self::generate(color.to_string(), Source::Color { value: color }, &colors, variant, mode, contrast)
    }

    /// Builds a theme from already-extracted source colours. This is the fast
    /// path used when switching variant, mode or contrast on a known wallpaper.
    pub fn generate(
        name: impl Into<String>,
        source: Source,
        colors: &SourceColors,
        variant: Variant,
        mode: Mode,
        contrast: i8,
    ) -> Result<Self, Error> {
        if !(-1..=1).contains(&contrast) {
            return Err(Error::BadContrast(contrast.into()));
        }
        Ok(Self {
            name: name.into(),
            source,
            variant,
            mode,
            contrast,
            roles: roles(colors, variant, mode, contrast),
            style: Style::default(),
            set_windows_accent: true,
        })
    }
}

/// Maps Material 3 dynamic colour onto Lumen's eight roles.
fn roles(colors: &SourceColors, variant: Variant, mode: Mode, contrast: i8) -> Roles {
    let dark = mode == Mode::Dark;
    let material_variant = match variant {
        Variant::Tonal => MaterialVariant::TonalSpot,
        Variant::Vibrant => MaterialVariant::Vibrant,
        Variant::Expressive => MaterialVariant::Expressive,
        Variant::Monochrome => MaterialVariant::Monochrome,
    };
    let scheme = DynamicScheme::by_variant(colors.seed().into(), &material_variant, dark, Some(f64::from(contrast)));

    Roles {
        primary: scheme.primary().into(),
        secondary: scheme.secondary().into(),
        tertiary: scheme.tertiary().into(),
        neutral: scheme.surface_container_high().into(),
        surface: scheme.surface().into(),
        on_surface: scheme.on_surface().into(),
        outline: scheme.outline().into(),
        accent: accent(colors, variant, dark, contrast),
    }
}

/// The accent is a highlight that stands apart from `primary`: the wallpaper's
/// most different second colour if it has one, otherwise the seed's complement.
/// Monochrome themes get a plain white/black highlight instead.
fn accent(colors: &SourceColors, variant: Variant, dark: bool, contrast: i8) -> Color {
    if variant == Variant::Monochrome {
        return if dark { Color::new(255, 255, 255) } else { Color::new(0, 0, 0) };
    }

    let seed = Hct::new(colors.seed().into());
    let distance = |hct: &Hct| {
        let d = (hct.get_hue() - seed.get_hue()).abs() % 360.0;
        d.min(360.0 - d)
    };
    let second = colors.candidates[1..]
        .iter()
        .map(|c| Hct::new((*c).into()))
        .filter(|hct| distance(hct) >= MIN_ACCENT_HUE_DISTANCE)
        .max_by(|a, b| distance(a).total_cmp(&distance(b)));

    let (hue, chroma) = match second {
        Some(hct) => (hct.get_hue(), hct.get_chroma().clamp(32.0, 64.0)),
        None => ((seed.get_hue() + 180.0) % 360.0, 48.0),
    };

    // Light on dark, dark on light; pushed further apart at high contrast.
    let tone = match (dark, contrast) {
        (true, 1) => 90,
        (true, _) => 80,
        (false, 1) => 30,
        (false, _) => 40,
    };
    TonalPalette::of(hue, chroma).tone(tone).into()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use material_colors::hct::Hct;

    use super::*;
    use crate::extract::{
        extract_from_image,
        tests::{harbour, hue, hue_distance},
    };

    fn harbour_colors() -> SourceColors {
        extract_from_image(&harbour(640, 360)).unwrap()
    }

    fn theme(colors: &SourceColors, variant: Variant, mode: Mode, contrast: i8) -> Theme {
        Theme::generate("Test", Source::Manual, colors, variant, mode, contrast).unwrap()
    }

    fn chroma(c: Color) -> f64 {
        Hct::new(c.into()).get_chroma()
    }

    #[test]
    fn every_variant_and_mode_is_readable() {
        let colors = harbour_colors();
        for &variant in Variant::ALL {
            for &mode in Mode::ALL {
                for contrast in -1..=1 {
                    let r = theme(&colors, variant, mode, contrast).roles;
                    let label = format!("{variant} {mode} contrast {contrast}");

                    // Body text on the window background: WCAG AA at least.
                    assert!(r.on_surface.contrast_ratio(r.surface) >= 4.5, "{label}: text on surface");
                    assert!(r.on_surface.contrast_ratio(r.neutral) >= 4.5, "{label}: text on neutral");
                    // Coloured roles are used for fills, icons and large text: 3:1.
                    for (name, c) in [("primary", r.primary), ("secondary", r.secondary), ("tertiary", r.tertiary), ("accent", r.accent)] {
                        assert!(c.contrast_ratio(r.surface) >= 3.0, "{label}: {name} {c} on surface {}", r.surface);
                    }
                    // Dark themes have dark surfaces, light themes light ones.
                    let surface_is_dark = r.surface.luminance() < 0.18;
                    assert_eq!(surface_is_dark, mode == Mode::Dark, "{label}: surface {}", r.surface);
                    // The raised neutral is visibly different from the surface.
                    assert_ne!(r.neutral, r.surface, "{label}");
                }
            }
        }
    }

    #[test]
    fn the_four_variants_give_four_different_palettes() {
        let colors = harbour_colors();
        for &mode in Mode::ALL {
            let distinct: HashSet<_> = Variant::ALL.iter().map(|&v| theme(&colors, v, mode, 0).roles).collect();
            assert_eq!(distinct.len(), Variant::ALL.len(), "{mode}");
        }
    }

    #[test]
    fn light_and_dark_differ() {
        let colors = harbour_colors();
        for &variant in Variant::ALL {
            assert_ne!(theme(&colors, variant, Mode::Light, 0).roles, theme(&colors, variant, Mode::Dark, 0).roles);
        }
    }

    #[test]
    fn tonal_primary_keeps_the_wallpaper_hue() {
        let colors = harbour_colors();
        for &mode in Mode::ALL {
            let primary = theme(&colors, Variant::Tonal, mode, 0).roles.primary;
            assert!(hue_distance(hue(primary), hue(colors.seed())) < 15.0, "{mode}: {primary}");
        }
    }

    #[test]
    fn vibrant_is_more_colourful_than_tonal() {
        // Compared in light mode: at the pale tone dark themes use for primary,
        // blue is already as saturated as sRGB allows, so the two can coincide.
        let colors = harbour_colors();
        let tonal = theme(&colors, Variant::Tonal, Mode::Light, 0).roles.primary;
        let vibrant = theme(&colors, Variant::Vibrant, Mode::Light, 0).roles.primary;
        assert!(chroma(vibrant) > chroma(tonal) + 10.0, "vibrant {vibrant} vs tonal {tonal}");
    }

    #[test]
    fn monochrome_has_no_colour() {
        let colors = harbour_colors();
        for &mode in Mode::ALL {
            let r = theme(&colors, Variant::Monochrome, mode, 0).roles;
            for c in [r.primary, r.secondary, r.tertiary, r.neutral, r.surface, r.on_surface, r.outline, r.accent] {
                assert!(chroma(c) < 5.0, "{mode}: {c} has chroma {}", chroma(c));
            }
        }
    }

    #[test]
    fn accent_comes_from_the_wallpapers_second_colour() {
        // The harbour is blue with a warm sun, so the accent should be warm.
        let accent = theme(&harbour_colors(), Variant::Tonal, Mode::Dark, 0).roles.accent;
        let sun = Color::new(0xF2, 0xCC, 0x8F);
        assert!(hue_distance(hue(accent), hue(sun)) < 20.0, "accent {accent}");
    }

    #[test]
    fn accent_falls_back_to_the_complement() {
        let seed = Color::new(0x2F, 0x5B, 0xEA);
        let r = Theme::from_color(seed, Variant::Tonal, Mode::Dark, 0).unwrap().roles;
        assert!(hue_distance(hue(r.accent), hue(seed)) > 120.0, "accent {} vs seed {seed}", r.accent);
    }

    #[test]
    fn higher_contrast_separates_text_from_surface() {
        let colors = harbour_colors();
        for &mode in Mode::ALL {
            let ratio = |contrast| {
                let r = theme(&colors, Variant::Tonal, mode, contrast).roles;
                r.outline.contrast_ratio(r.surface)
            };
            assert!(ratio(1) > ratio(0), "{mode}: high should exceed standard");
            assert!(ratio(0) > ratio(-1), "{mode}: standard should exceed reduced");
        }
    }

    #[test]
    fn contrast_outside_the_slider_is_rejected() {
        let colors = harbour_colors();
        for bad in [-2, 2, 100] {
            let result = Theme::generate("Test", Source::Manual, &colors, Variant::Tonal, Mode::Dark, bad);
            assert!(matches!(result, Err(Error::BadContrast(_))), "{bad}");
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let colors = harbour_colors();
        assert_eq!(theme(&colors, Variant::Expressive, Mode::Light, 0), theme(&colors, Variant::Expressive, Mode::Light, 0));
    }

    #[test]
    fn from_color_records_its_source() {
        let seed = Color::new(0x8E, 0xA6, 0xFF);
        let theme = Theme::from_color(seed, Variant::Vibrant, Mode::Light, 0).unwrap();
        assert_eq!(theme.source, Source::Color { value: seed });
        assert_eq!(theme.name, "#8EA6FF");
        assert_eq!(theme.variant, Variant::Vibrant);
        assert_eq!(theme.mode, Mode::Light);
    }

    #[test]
    fn theme_json_has_the_planned_shape() {
        let theme = Theme::generate(
            "Midnight Harbour",
            Source::Wallpaper { path: "wallpapers/midnight-harbour.jpg".into() },
            &harbour_colors(),
            Variant::Tonal,
            Mode::Dark,
            0,
        )
        .unwrap();
        let json = serde_json::to_value(&theme).unwrap();

        let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<HashSet<_>>();
        let set = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        assert_eq!(keys(&json), set(&["name", "source", "variant", "mode", "contrast", "roles", "style", "setWindowsAccent"]));
        assert_eq!(keys(&json["roles"]), set(&["primary", "secondary", "tertiary", "neutral", "surface", "onSurface", "outline", "accent"]));
        assert_eq!(keys(&json["style"]), set(&["radius", "blur", "border", "font", "motion"]));
        assert_eq!(json["source"], serde_json::json!({ "type": "wallpaper", "path": "wallpapers/midnight-harbour.jpg" }));
        assert_eq!(json["variant"], "tonal");
        assert_eq!(json["mode"], "dark");
        assert_eq!(json["contrast"], 0);
        for (role, value) in json["roles"].as_object().unwrap() {
            let hex = value.as_str().unwrap();
            assert!(hex.len() == 7 && hex.starts_with('#'), "{role}: {hex}");
        }

        // And it reads back to the same theme.
        assert_eq!(serde_json::from_value::<Theme>(json).unwrap(), theme);
    }
}
