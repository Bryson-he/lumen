//! The Windows accent colour.
//!
//! Windows has no public API for setting the accent, so this writes the same
//! registry values Windows Settings writes and then broadcasts the change.
//! It is optional in Lumen and should be re-tested after Windows feature updates.
//! Take a [`snapshot`] first so the previous accent can be put back exactly.

use lumen_theme::Color;
use windows_registry::{Type, Value};

use crate::{
    Error, Result,
    util::{broadcast_color_change, hkcu, read_u32, read_value, restore_value, write_u32},
};

const DWM: &str = r"Software\Microsoft\Windows\DWM";
const EXPLORER_ACCENT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
const DESKTOP: &str = r"Control Panel\Desktop";

const DWM_VALUES: [&str; 3] = ["AccentColor", "ColorizationColor", "ColorizationAfterglow"];
const EXPLORER_VALUES: [&str; 3] = ["AccentColorMenu", "StartColorMenu", "AccentPalette"];
const AUTO_COLORIZATION: &str = "AutoColorization";

/// Every registry value [`set`] touches, as it was before. Pass to [`restore`].
#[derive(Debug, Clone)]
pub struct Snapshot {
    dwm: [Option<Value>; 3],
    explorer: [Option<Value>; 3],
    auto_colorization: Option<Value>,
}

/// The current accent colour, if Windows has one recorded.
pub fn get() -> Result<Option<Color>> {
    let explorer = hkcu(EXPLORER_ACCENT)?;
    let dwm = hkcu(DWM)?;
    let abgr = read_u32(&explorer, "AccentColorMenu").or_else(|| read_u32(&dwm, "AccentColor"));
    Ok(abgr.map(from_abgr))
}

/// Records the current accent settings so they can be restored later.
pub fn snapshot() -> Result<Snapshot> {
    let dwm = hkcu(DWM)?;
    let explorer = hkcu(EXPLORER_ACCENT)?;
    let desktop = hkcu(DESKTOP)?;
    Ok(Snapshot {
        dwm: DWM_VALUES.map(|name| read_value(&dwm, name)),
        explorer: EXPLORER_VALUES.map(|name| read_value(&explorer, name)),
        auto_colorization: read_value(&desktop, AUTO_COLORIZATION),
    })
}

/// Sets the Windows accent colour (taskbar, Start, title bars, app highlights).
///
/// Also turns off "pick an accent colour from my background", which would
/// otherwise replace this colour the next time the wallpaper changes.
pub fn set(color: Color) -> Result<()> {
    let dwm = hkcu(DWM)?;
    let explorer = hkcu(EXPLORER_ACCENT)?;
    let desktop = hkcu(DESKTOP)?;
    let shades = shades(color);

    write_u32(&desktop, AUTO_COLORIZATION, 0)?;
    write_u32(&dwm, "AccentColor", abgr(color))?;
    write_u32(&dwm, "ColorizationColor", argb(color, 0xC4))?;
    write_u32(&dwm, "ColorizationAfterglow", argb(color, 0xC4))?;
    write_u32(&explorer, "AccentColorMenu", abgr(color))?;
    write_u32(&explorer, "StartColorMenu", abgr(shades[5]))?;
    let previous = read_value(&explorer, "AccentPalette");
    explorer
        .set_bytes("AccentPalette", Type::Bytes, &palette_bytes(&shades, previous.as_deref()))
        .map_err(|e| Error::Registry { what: "AccentPalette", message: e.to_string() })?;

    broadcast_color_change();
    Ok(())
}

/// Puts back the accent settings captured by [`snapshot`].
pub fn restore(snapshot: &Snapshot) -> Result<()> {
    let dwm = hkcu(DWM)?;
    let explorer = hkcu(EXPLORER_ACCENT)?;
    let desktop = hkcu(DESKTOP)?;
    for (name, value) in DWM_VALUES.into_iter().zip(&snapshot.dwm) {
        restore_value(&dwm, name, value.as_ref())?;
    }
    for (name, value) in EXPLORER_VALUES.into_iter().zip(&snapshot.explorer) {
        restore_value(&explorer, name, value.as_ref())?;
    }
    restore_value(&desktop, AUTO_COLORIZATION, snapshot.auto_colorization.as_ref())?;
    broadcast_color_change();
    Ok(())
}

/// The seven shades Windows keeps for an accent: three lighter, the accent
/// itself, three darker (the layout of `AccentPalette`).
fn shades(color: Color) -> [Color; 7] {
    let mix = |target: u8, amount: f64| {
        let channel = |c: u8| (f64::from(c) + (f64::from(target) - f64::from(c)) * amount).round() as u8;
        Color::new(channel(color.r), channel(color.g), channel(color.b))
    };
    [mix(255, 0.75), mix(255, 0.5), mix(255, 0.25), color, mix(0, 0.25), mix(0, 0.5), mix(0, 0.7)]
}

/// `AccentPalette` is eight 4-byte entries (red, green, blue, 0): the seven
/// shades plus one extra colour Windows uses for a few legacy highlights.
/// That eighth entry is kept from the `previous` palette when there is one.
fn palette_bytes(shades: &[Color; 7], previous: Option<&[u8]>) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (slot, shade) in bytes.chunks_exact_mut(4).zip(shades) {
        slot.copy_from_slice(&[shade.r, shade.g, shade.b, 0]);
    }
    match previous {
        Some(previous) if previous.len() == 32 => bytes[28..].copy_from_slice(&previous[28..]),
        // Windows' own default.
        _ => bytes[28..].copy_from_slice(&[0x88, 0x17, 0x98, 0x00]),
    }
    bytes
}

const fn abgr(c: Color) -> u32 {
    0xFF00_0000 | (c.b as u32) << 16 | (c.g as u32) << 8 | c.r as u32
}

const fn argb(c: Color, alpha: u8) -> u32 {
    (alpha as u32) << 24 | (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
}

const fn from_abgr(v: u32) -> Color {
    Color::new(v as u8, (v >> 8) as u8, (v >> 16) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Color = Color::new(0x2F, 0x5B, 0xEA);

    #[test]
    fn colour_packing_round_trips() {
        assert_eq!(abgr(BLUE), 0xFFEA_5B2F);
        assert_eq!(argb(BLUE, 0xC4), 0xC42F_5BEA);
        assert_eq!(from_abgr(abgr(BLUE)), BLUE);
    }

    #[test]
    fn shades_run_light_to_dark_around_the_accent() {
        let shades = shades(BLUE);
        assert_eq!(shades[3], BLUE);
        for pair in shades.windows(2) {
            assert!(pair[0].luminance() > pair[1].luminance(), "{} should be lighter than {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn palette_has_the_accent_in_the_fourth_slot() {
        let bytes = palette_bytes(&shades(BLUE), None);
        assert_eq!(bytes[12..16], [0x2F, 0x5B, 0xEA, 0x00]);
        assert_eq!(bytes[28..], [0x88, 0x17, 0x98, 0x00]);

        // An existing palette keeps its eighth entry.
        let mut previous = [0u8; 32];
        previous[28..].copy_from_slice(&[0xF7, 0x63, 0x0C, 0x00]);
        assert_eq!(palette_bytes(&shades(BLUE), Some(&previous))[28..], [0xF7, 0x63, 0x0C, 0x00]);
        assert_eq!(bytes.len(), 32);
    }
}
