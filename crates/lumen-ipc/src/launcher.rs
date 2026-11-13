//! The launcher: the shape of `launcher.json` and of the launcher messages
//! between the core and the settings app.

use serde::{Deserialize, Serialize};

/// The key combination that opens the launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LauncherHotkey {
    #[default]
    AltSpace,
    CtrlSpace,
    WinAltSpace,
}

/// `launcher.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherConfig {
    pub enabled: bool,
    pub hotkey: LauncherHotkey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults_and_names_are_camel_case() {
        let config: LauncherConfig = serde_json::from_str(r#"{ "enabled": true }"#).unwrap();
        assert_eq!(config, LauncherConfig { enabled: true, hotkey: LauncherHotkey::AltSpace });
        assert!(!LauncherConfig::default().enabled, "off until switched on");
        assert_eq!(serde_json::to_value(LauncherHotkey::WinAltSpace).unwrap(), "winAltSpace");
    }
}
