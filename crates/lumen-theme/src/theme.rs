use std::{fmt, path::Path, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{Color, Error};

/// A Lumen theme file (`%APPDATA%\Lumen\themes\*.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub name: String,
    pub source: Source,
    pub variant: Variant,
    pub mode: Mode,
    /// -1 reduced, 0 standard, 1 high.
    pub contrast: i8,
    pub roles: Roles,
    pub style: Style,
    pub set_windows_accent: bool,
}

/// Where the theme's colours came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    /// Generated from a wallpaper image.
    Wallpaper { path: String },
    /// Generated from one picked colour.
    Color { value: Color },
    /// Every role set by hand.
    Manual,
}

/// The eight colour roles every Lumen surface reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Roles {
    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,
    /// A raised neutral, one step up from `surface`.
    pub neutral: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub outline: Color,
    /// A highlight that stands apart from `primary`, taken from a second
    /// colour in the wallpaper when there is one.
    pub accent: Color,
}

/// Non-colour style tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Corner radius in px (0–24).
    pub radius: u8,
    pub blur: Blur,
    /// Border width in px (0–3).
    pub border: u8,
    pub font: String,
    pub motion: Motion,
}

impl Default for Style {
    fn default() -> Self {
        Self { radius: 12, blur: Blur::Medium, border: 1, font: "Inter".to_owned(), motion: Motion::Normal }
    }
}

/// Declares a lowercase string enum with `FromStr`, `Display` and serde.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident, $what:literal, $expected:literal { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Error> {
                match s.to_ascii_lowercase().as_str() {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(Error::BadOption { what: $what, value: s.to_owned(), expected: $expected }),
                }
            }
        }
    };
}

string_enum! {
    /// How colourful the scheme is.
    Variant, "variant", "tonal, vibrant, expressive, monochrome" {
        Tonal => "tonal",
        Vibrant => "vibrant",
        Expressive => "expressive",
        Monochrome => "monochrome",
    }
}

string_enum! {
    /// Light or dark. "Auto" is resolved to one of these by the core before generating.
    Mode, "mode", "light, dark" { Light => "light", Dark => "dark" }
}

string_enum! {
    /// Blur / glass strength.
    Blur, "blur", "none, low, medium, high" { None => "none", Low => "low", Medium => "medium", High => "high" }
}

string_enum! {
    /// Animation speed.
    Motion, "motion", "slow, normal, fast" { Slow => "slow", Normal => "normal", Fast => "fast" }
}

/// Turns a wallpaper path into a display name: `midnight-harbour.jpg` -> `Midnight Harbour`.
pub(crate) fn name_from_path(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    let name = stem
        .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
            first + chars.as_str()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() { "Untitled".to_owned() } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_file_names() {
        assert_eq!(name_from_path(Path::new("wallpapers/midnight-harbour.jpg")), "Midnight Harbour");
        assert_eq!(name_from_path(Path::new(r"C:\Pics\neon_city  4k.png")), "Neon City 4k");
        assert_eq!(name_from_path(Path::new("")), "Untitled");
    }

    #[test]
    fn options_parse_and_reject() {
        assert_eq!("Tonal".parse::<Variant>().unwrap(), Variant::Tonal);
        assert_eq!("monochrome".parse::<Variant>().unwrap(), Variant::Monochrome);
        assert_eq!("dark".parse::<Mode>().unwrap(), Mode::Dark);
        let err = "neon".parse::<Variant>().unwrap_err().to_string();
        assert!(err.contains("tonal, vibrant, expressive, monochrome"), "{err}");
    }

    #[test]
    fn expected_lists_match_the_variants() {
        let joined = |all: &[&str]| all.join(", ");
        assert!("x".parse::<Variant>().unwrap_err().to_string().contains(&joined(&Variant::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>())));
        assert!("x".parse::<Mode>().unwrap_err().to_string().contains(&joined(&Mode::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>())));
        assert!("x".parse::<Blur>().unwrap_err().to_string().contains(&joined(&Blur::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>())));
        assert!("x".parse::<Motion>().unwrap_err().to_string().contains(&joined(&Motion::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>())));
    }

    #[test]
    fn reads_the_example_theme_from_the_plan() {
        let json = r##"{
          "name": "Midnight Harbour",
          "source": { "type": "wallpaper", "path": "wallpapers/midnight-harbour.jpg" },
          "variant": "tonal", "mode": "dark", "contrast": 0,
          "roles": { "primary": "#8EA6FF", "secondary": "#B7C3E8", "tertiary": "#D9A7C7",
                     "neutral": "#2A2D35", "surface": "#1C1D21", "onSurface": "#ECECEA",
                     "outline": "#4A4E58", "accent": "#F2CC8F" },
          "style": { "radius": 12, "blur": "medium", "border": 1, "font": "Inter", "motion": "normal" },
          "setWindowsAccent": true
        }"##;
        let theme: Theme = serde_json::from_str(json).unwrap();
        assert_eq!(theme.name, "Midnight Harbour");
        assert_eq!(theme.source, Source::Wallpaper { path: "wallpapers/midnight-harbour.jpg".into() });
        assert_eq!(theme.roles.on_surface.to_string(), "#ECECEA");
        assert_eq!(theme.style, Style::default());

        // Writing it back gives the same JSON value, key for key.
        let original: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_value(&theme).unwrap(), original);
    }
}
