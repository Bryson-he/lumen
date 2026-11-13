//! Flyouts: the shape of `flyouts.json` and of the flyout messages between
//! the core and the settings app.

use serde::{Deserialize, Serialize};

/// How a flyout looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlyoutStyle {
    /// One compact line: label, level, percentage.
    #[default]
    Pill,
    /// Taller, with what is playing underneath.
    Card,
}

/// Where on the screen a flyout appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlyoutPosition {
    TopLeft,
    TopCentre,
    TopRight,
    BottomLeft,
    #[default]
    BottomCentre,
    BottomRight,
}

/// `flyouts.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlyoutConfig {
    /// Show Lumen's volume flyout, and answer the volume keys in place of Windows.
    pub volume: bool,
    pub style: FlyoutStyle,
    pub position: FlyoutPosition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_defaults_and_names_are_camel_case() {
        let config: FlyoutConfig = serde_json::from_str(r#"{ "volume": true, "position": "topRight" }"#).unwrap();
        assert_eq!(config, FlyoutConfig { volume: true, style: FlyoutStyle::Pill, position: FlyoutPosition::TopRight });
        assert!(!FlyoutConfig::default().volume, "off until switched on");
        assert_eq!(serde_json::to_value(FlyoutConfig::default()).unwrap()["position"], "bottomCentre");
    }
}
