//! The real desktop, behind a trait so the service can be tested without
//! changing anyone's wallpaper.

use std::path::PathBuf;

use lumen_theme::{Color, Mode};
use lumen_win::{
    accent,
    appearance::{self, Appearance},
    wallpaper::Wallpapers,
};

/// What to put on the desktop for a theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPlan {
    /// `(monitor id, image)`; a `None` id means every monitor.
    pub wallpapers: Vec<(Option<String>, PathBuf)>,
    pub mode: Mode,
    /// The Windows accent colour, when the theme asks for it to be set.
    pub accent: Option<Color>,
}

pub trait Desktop {
    /// Everything [`Desktop::apply`] might change, as it is right now.
    type Snapshot;

    fn snapshot(&mut self) -> Result<Self::Snapshot, String>;
    fn apply(&mut self, plan: &ApplyPlan) -> Result<(), String>;
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), String>;
}

/// The Windows desktop, through `lumen-win`.
pub struct WindowsDesktop {
    wallpapers: Wallpapers,
    /// Whether a preview has written the accent since the last snapshot, so a
    /// revert only touches the accent when there is something to undo.
    accent_changed: bool,
}

pub struct WindowsSnapshot {
    wallpapers: Vec<(String, Option<PathBuf>)>,
    appearance: Appearance,
    accent: accent::Snapshot,
}

impl WindowsDesktop {
    /// Must be created on the thread that will use it (it holds a COM object).
    pub fn new() -> Result<Self, String> {
        Ok(Self { wallpapers: Wallpapers::new().map_err(text)?, accent_changed: false })
    }
}

impl Desktop for WindowsDesktop {
    type Snapshot = WindowsSnapshot;

    fn snapshot(&mut self) -> Result<WindowsSnapshot, String> {
        self.accent_changed = false;
        Ok(WindowsSnapshot {
            wallpapers: self.wallpapers.monitors().map_err(text)?.into_iter().map(|m| (m.id, m.wallpaper)).collect(),
            appearance: appearance::get().map_err(text)?,
            accent: accent::snapshot().map_err(text)?,
        })
    }

    fn apply(&mut self, plan: &ApplyPlan) -> Result<(), String> {
        if !plan.wallpapers.is_empty() {
            let monitors = self.wallpapers.monitors().map_err(text)?;
            for (target, image) in &plan.wallpapers {
                // Skip monitors already showing this image, to avoid a needless fade.
                let already = monitors
                    .iter()
                    .filter(|m| target.as_ref().is_none_or(|id| *id == m.id))
                    .all(|m| m.wallpaper.as_deref() == Some(image.as_path()));
                if !already {
                    self.wallpapers.set_by_id(target.as_deref(), image).map_err(text)?;
                }
            }
        }
        appearance::set(Appearance::both(plan.mode)).map_err(text)?;
        if let Some(color) = plan.accent
            && accent::get().map_err(text)? != Some(color)
        {
            self.accent_changed = true;
            accent::set(color).map_err(text)?;
        }
        Ok(())
    }

    fn restore(&mut self, snapshot: &WindowsSnapshot) -> Result<(), String> {
        let now = self.wallpapers.monitors().map_err(text)?;
        for (id, image) in &snapshot.wallpapers {
            let current = now.iter().find(|m| m.id == *id).and_then(|m| m.wallpaper.as_ref());
            if let Some(image) = image.as_ref().filter(|image| Some(*image) != current) {
                self.wallpapers.set_by_id(Some(id), image).map_err(text)?;
            }
        }
        appearance::set(snapshot.appearance).map_err(text)?;
        if std::mem::take(&mut self.accent_changed) {
            accent::restore(&snapshot.accent).map_err(text)?;
        }
        Ok(())
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
