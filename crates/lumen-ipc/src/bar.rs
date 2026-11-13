//! The status bar: the shape of `bar.json` and of the bar messages between
//! the core and the settings app.

use serde::{Deserialize, Serialize};

/// Bar height limits, in logical pixels.
pub const BAR_HEIGHT_RANGE: (u8, u8) = (28, 56);
/// The most apps one launcher module holds.
pub const MAX_LAUNCHER_APPS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
}

/// One program in a launcher module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherApp {
    pub name: String,
    /// Full path of the program or shortcut to open.
    pub path: String,
}

/// Something shown in a slot of the bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BarModule {
    /// The Lumen name; click opens the settings app.
    Lumen,
    Clock,
    Date,
    Cpu,
    Memory,
    Gpu,
    /// What is playing; click plays or pauses.
    Media,
    /// The temperature and conditions, once weather is set up.
    Weather,
    /// A row of program icons; click opens one.
    Launcher {
        apps: Vec<LauncherApp>,
    },
}

/// `bar.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BarConfig {
    pub enabled: bool,
    pub position: BarPosition,
    /// Height in logical pixels.
    pub height: u8,
    /// Which monitors show the bar: `"main"`, `"all"`, or `"#2"` for monitor number 2.
    pub monitor: String,
    /// Reserve the bar's strip of the screen, so maximised windows stop at it.
    /// When off, the bar floats over whatever is underneath.
    pub reserve_space: bool,
    pub left: Vec<BarModule>,
    pub centre: Vec<BarModule>,
    pub right: Vec<BarModule>,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: BarPosition::Top,
            height: 36,
            monitor: "main".to_owned(),
            reserve_space: true,
            left: vec![BarModule::Lumen, BarModule::Media],
            centre: vec![BarModule::Clock],
            right: vec![BarModule::Cpu, BarModule::Memory, BarModule::Date],
        }
    }
}

impl BarConfig {
    /// Keeps hand-edited or imported files within sensible limits.
    pub fn sanitised(mut self) -> Self {
        self.height = self.height.clamp(BAR_HEIGHT_RANGE.0, BAR_HEIGHT_RANGE.1);
        if self.monitor != "main" && self.monitor != "all" && crate::presets::monitor_number(&self.monitor).is_none() {
            self.monitor = "main".to_owned();
        }
        for slot in [&mut self.left, &mut self.centre, &mut self.right] {
            slot.truncate(12);
            for module in slot.iter_mut() {
                if let BarModule::Launcher { apps } = module {
                    apps.retain(|app| !app.path.trim().is_empty());
                    apps.truncate(MAX_LAUNCHER_APPS);
                }
            }
        }
        self
    }

    /// Every module, in left / centre / right order.
    pub fn modules(&self) -> impl Iterator<Item = &BarModule> {
        self.left.iter().chain(&self.centre).chain(&self.right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_off_until_turned_on() {
        let config = BarConfig::default();
        assert!(!config.enabled);
        assert!(config.reserve_space);
        assert_eq!(config.modules().count(), 6);
    }

    #[test]
    fn config_round_trips_with_readable_names() {
        let mut config = BarConfig { enabled: true, position: BarPosition::Bottom, monitor: "#2".into(), ..Default::default() };
        config.centre = vec![BarModule::Launcher { apps: vec![LauncherApp { name: "Notepad".into(), path: r"C:\Windows\notepad.exe".into() }] }];
        let text = serde_json::to_string(&config).unwrap();
        assert!(text.contains(r#""position":"bottom""#) && text.contains(r#""type":"launcher""#) && text.contains(r#""reserveSpace":true"#), "{text}");
        assert_eq!(serde_json::from_str::<BarConfig>(&text).unwrap(), config);
    }

    #[test]
    fn a_partial_file_is_filled_in_and_wild_values_are_tamed() {
        let config: BarConfig = serde_json::from_str(r#"{"enabled":true,"height":200,"monitor":"that one"}"#).unwrap();
        let config = config.sanitised();
        assert!(config.enabled);
        assert_eq!(config.height, BAR_HEIGHT_RANGE.1);
        assert_eq!(config.monitor, "main");
        assert_eq!(config.left, BarConfig::default().left, "missing slots take the defaults");

        let many = BarConfig { right: vec![BarModule::Clock; 40], ..Default::default() }.sanitised();
        assert_eq!(many.right.len(), 12);
    }
}
