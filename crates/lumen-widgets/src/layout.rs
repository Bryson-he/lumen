//! Reading and writing `widgets/layout.json`. The layout's shape is defined
//! in `lumen-ipc`, because the settings app sees it too.

use std::{fs, path::Path};

pub use lumen_ipc::widgets::{Background, Layout, Placement, WidgetKind as Kind, WidgetStyle};

/// Reads the layout, creating the default one on first run. A file that
/// cannot be read is left alone and the default is used for this session.
pub fn load_or_create(path: &Path) -> Layout {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Layout>(&text).map(Layout::sanitised).unwrap_or_else(|error| {
            tracing::warn!("{} is not a valid layout ({error}); using the default", path.display());
            Layout::default()
        }),
        Err(_) => {
            let layout = Layout::default();
            if let Err(error) = save(path, &layout) {
                tracing::warn!("{error}");
            }
            layout
        }
    }
}

/// Writes the layout through a temporary file, so a crash never leaves half a file.
pub fn save(path: &Path, layout: &Layout) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("could not save {}: {error}", path.display());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    let text = serde_json::to_string_pretty(layout).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).and_then(|()| fs::rename(&temp, path)).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lumen-layout-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("widgets").join("layout.json")
    }

    #[test]
    fn first_run_writes_the_file_and_later_runs_read_it() {
        let path = temp("first");
        let created = load_or_create(&path);
        assert!(path.is_file());
        assert_eq!(created, Layout::default());

        // Move the clock, as edit mode does; the change is what comes back.
        let mut edited = created;
        edited.widgets[0].x = 408.0;
        edited.locked = true;
        save(&path, &edited).unwrap();
        assert_eq!(load_or_create(&path), edited);
        assert!(!path.with_extension("json.tmp").exists(), "no temporary file left behind");
    }

    #[test]
    fn a_broken_file_falls_back_without_being_overwritten() {
        let path = temp("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_or_create(&path), Layout::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
    }
}
