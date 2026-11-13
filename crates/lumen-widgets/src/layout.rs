//! Which widgets are on the desktop and where (`widgets/layout.json`).

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{clock, media, stats};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Clock,
    SystemStats,
    NowPlaying,
}

impl Kind {
    /// Default size in logical pixels.
    pub const fn default_size(self) -> (f32, f32) {
        match self {
            Self::Clock => clock::SIZE,
            Self::SystemStats => stats::SIZE,
            Self::NowPlaying => media::SIZE,
        }
    }
}

/// One widget on the desktop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub kind: Kind,
    /// The monitor it is on (Windows' monitor id); the primary monitor if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Top-left corner, in logical pixels from the top-left of that monitor's work area.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub widgets: Vec<Placement>,
}

impl Default for Layout {
    /// The first-run layout: clock, system stats and now playing down the left edge.
    fn default() -> Self {
        let mut y = 48.0;
        let widgets = [(Kind::Clock, "clock-1"), (Kind::SystemStats, "system-1"), (Kind::NowPlaying, "now-playing-1")]
            .into_iter()
            .map(|(kind, id)| {
                let (width, height) = kind.default_size();
                let placement = Placement { id: id.to_owned(), kind, monitor: None, x: 48.0, y, width, height };
                y += height + 24.0;
                placement
            })
            .collect();
        Self { widgets }
    }
}

impl Layout {
    /// Reads the layout, creating the default one on first run. A file that
    /// cannot be read is left alone and the default is used for this session.
    pub fn load_or_create(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Self>(&text).map(Self::sanitised).unwrap_or_else(|error| {
                tracing::warn!("{} is not a valid layout ({error}); using the default", path.display());
                Self::default()
            }),
            Err(_) => {
                let layout = Self::default();
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(text) = serde_json::to_string_pretty(&layout) {
                    let _ = fs::write(path, text);
                }
                layout
            }
        }
    }

    /// Keeps hand-edited files from producing absurd windows.
    fn sanitised(mut self) -> Self {
        self.widgets.retain(|w| w.x.is_finite() && w.y.is_finite() && w.width.is_finite() && w.height.is_finite());
        for widget in &mut self.widgets {
            widget.width = widget.width.clamp(48.0, 2000.0);
            widget.height = widget.height.clamp(48.0, 2000.0);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_the_three_widgets_without_overlap() {
        let layout = Layout::default();
        assert_eq!(layout.widgets.iter().map(|w| w.kind).collect::<Vec<_>>(), [Kind::Clock, Kind::SystemStats, Kind::NowPlaying]);
        for pair in layout.widgets.windows(2) {
            assert!(pair[0].y + pair[0].height < pair[1].y, "{} overlaps {}", pair[0].id, pair[1].id);
        }
    }

    #[test]
    fn first_run_writes_the_file_and_later_runs_read_it() {
        let path = std::env::temp_dir().join(format!("lumen-layout-test-{}", std::process::id())).join("widgets/layout.json");
        let _ = fs::remove_file(&path);
        let created = Layout::load_or_create(&path);
        assert!(path.is_file());
        assert_eq!(created, Layout::default());

        // Move the clock, as edit mode will; the change is what comes back.
        let mut edited = created;
        edited.widgets[0].x = 400.0;
        fs::write(&path, serde_json::to_string(&edited).unwrap()).unwrap();
        assert_eq!(Layout::load_or_create(&path), edited);

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"kind\":\"clock\"") && text.contains("\"kind\":\"systemStats\"") && text.contains("\"kind\":\"nowPlaying\""), "{text}");
    }

    #[test]
    fn a_broken_file_falls_back_and_wild_sizes_are_clamped() {
        let dir = std::env::temp_dir().join(format!("lumen-layout-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("layout.json");
        fs::write(&path, "not json").unwrap();
        assert_eq!(Layout::load_or_create(&path), Layout::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json", "the user's file is not overwritten");

        fs::write(&path, r#"{"widgets":[{"id":"a","kind":"clock","x":0,"y":0,"width":1,"height":99999}]}"#).unwrap();
        let layout = Layout::load_or_create(&path);
        assert_eq!((layout.widgets[0].width, layout.widgets[0].height), (48.0, 2000.0));
    }
}
