use std::{fmt, str::FromStr};

use material_colors::color::Rgb;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Error;

/// An opaque sRGB colour. Serialises as `"#RRGGBB"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// WCAG relative luminance, 0.0 (black) to 1.0 (white).
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG contrast ratio against another colour, 1.0 to 21.0.
    pub fn contrast_ratio(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let bad = || Error::BadColor(s.to_owned());
        let hex = s.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        Ok(Self::new(byte(0)?, byte(2)?, byte(4)?))
    }
}

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Self {
        Self::new(c.red, c.green, c.blue)
    }
}

impl From<Color> for Rgb {
    fn from(c: Color) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c: Color = "#8EA6FF".parse().unwrap();
        assert_eq!(c, Color::new(0x8E, 0xA6, 0xFF));
        assert_eq!(c.to_string(), "#8EA6FF");
        assert_eq!("8ea6ff".parse::<Color>().unwrap(), c);
    }

    #[test]
    fn rejects_bad_hex() {
        for bad in ["", "#fff", "#GGGGGG", "#8EA6FF00", "blue", "#ééé", "+8EA6F"] {
            assert!(bad.parse::<Color>().is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        let ratio = Color::new(0, 0, 0).contrast_ratio(Color::new(255, 255, 255));
        assert!((ratio - 21.0).abs() < 0.01);
    }

    #[test]
    fn serialises_as_hex_string() {
        let json = serde_json::to_string(&Color::new(0x1C, 0x1D, 0x21)).unwrap();
        assert_eq!(json, "\"#1C1D21\"");
        assert_eq!(serde_json::from_str::<Color>(&json).unwrap(), Color::new(0x1C, 0x1D, 0x21));
    }
}
