//! Windows light / dark mode.

use lumen_theme::Mode;

use crate::{
    Result,
    util::{broadcast_color_change, hkcu, read_u32, write_u32},
};

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS: &str = "AppsUseLightTheme";
const SYSTEM: &str = "SystemUsesLightTheme";

/// Windows keeps two switches: one for apps, one for the taskbar and Start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appearance {
    pub apps: Mode,
    pub system: Mode,
}

impl Appearance {
    /// Both switches set the same way.
    pub const fn both(mode: Mode) -> Self {
        Self { apps: mode, system: mode }
    }
}

/// Reads the current light/dark setting. Missing values mean light, as in Windows.
pub fn get() -> Result<Appearance> {
    let key = hkcu(PERSONALIZE)?;
    let mode = |name| if read_u32(&key, name) == Some(0) { Mode::Dark } else { Mode::Light };
    Ok(Appearance { apps: mode(APPS), system: mode(SYSTEM) })
}

/// Switches Windows to light or dark and tells running apps to update.
/// Does nothing if the setting is already as requested.
pub fn set(appearance: Appearance) -> Result<()> {
    if get()? == appearance {
        return Ok(());
    }
    let key = hkcu(PERSONALIZE)?;
    let flag = |mode| u32::from(mode == Mode::Light);
    write_u32(&key, APPS, flag(appearance.apps))?;
    write_u32(&key, SYSTEM, flag(appearance.system))?;
    broadcast_color_change();
    Ok(())
}
