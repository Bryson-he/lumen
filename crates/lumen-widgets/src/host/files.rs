//! Widget files in the host: reading the YAML files in the widgets folder,
//! reading them again the moment one is saved, and writing a file when the
//! designer changes a widget that comes from one.
//!
//! A file with a mistake never blanks a widget: the last version that read
//! correctly stays on screen, and the mistake is reported.

use std::path::{Path, PathBuf};

use lumen_ipc::{
    parts::Part,
    widget_file::{self, WidgetFileInfo},
};
use lumen_win::watch::{FolderWatch, watch_folder};

use super::*;

/// Posted to the host's window when something in the widgets folder changes.
pub(super) const FILES_CHANGED: u32 = WM_APP + 13;
pub(super) const TIMER_FILES: usize = 7;
/// A save often arrives as several changes; wait for them to settle.
const SETTLE_MS: u32 = 120;

#[derive(Default)]
pub(super) struct Files {
    /// Every `.yaml` file in the widgets folder, by file name.
    pub(super) list: Vec<WidgetFileInfo>,
    /// Parts being dragged in the designer, not yet written to their file.
    drafts: HashMap<String, Vec<Part>>,
    watch: Option<FolderWatch>,
}

impl Host {
    /// Reads the folder and starts watching it.
    pub(super) fn start_files(&mut self) {
        let folder = self.options.widget_files_dir.clone();
        let _ = std::fs::create_dir_all(&folder);
        self.files.list = read_folder(&folder, &[]);
        self.files.watch = watch_folder(&folder, self.window.0 as isize, FILES_CHANGED);
        if self.files.watch.is_none() {
            warn!("could not watch {}; widget files will not reload by themselves", folder.display());
        }
    }

    /// Something in the folder changed: read it again shortly.
    pub(super) fn on_files_changed(&mut self) {
        unsafe { SetTimer(Some(self.window), TIMER_FILES, SETTLE_MS, None) };
    }

    /// Handles the settle timer; `false` if the timer is not this one.
    pub(super) fn on_files_timer(&mut self, timer: usize) -> bool {
        if timer != TIMER_FILES {
            return false;
        }
        unsafe {
            let _ = KillTimer(Some(self.window), TIMER_FILES);
        }
        self.reload_files();
        true
    }

    /// Reads every file again; if anything is different, redraws and reports.
    pub(super) fn reload_files(&mut self) {
        let fresh = read_folder(&self.options.widget_files_dir, &self.files.list);
        if fresh == self.files.list {
            return;
        }
        for file in &fresh {
            let before = self.files.list.iter().find(|old| old.file == file.file);
            if file.error.is_some() && before.is_none_or(|old| old.error != file.error) {
                warn!("{}: {}", file.file, file.error.as_deref().unwrap_or_default());
            }
        }
        self.files.list = fresh;
        self.draw(None);
        self.sync_timers();
        self.sync_edit_surfaces();
        (self.on_state)(Report::Files(self.files.list.clone()));
    }

    fn file(&self, name: &str) -> Option<&WidgetFileInfo> {
        self.files.list.iter().find(|file| file.file == name && file.is_widget)
    }

    /// What a widget is made of, and the size those parts are laid out for:
    /// its file's parts, else its own, else the ones its kind comes with.
    pub(super) fn parts_of(&self, placement: &Placement) -> (Vec<Part>, (f32, f32)) {
        if let Some(name) = &placement.file {
            if let Some(file) = self.file(name) {
                let parts = self.files.drafts.get(name).unwrap_or(&file.parts).clone();
                return (parts, (file.size[0], file.size[1]));
            }
            // The file is gone: nothing to draw but the card.
            return (Vec::new(), placement.kind.default_size());
        }
        (placement.parts.clone().unwrap_or_else(|| lumen_ipc::parts::default_parts(placement.kind)), placement.kind.default_size())
    }

    /// The size a new widget made from this file starts at.
    pub(super) fn file_size(&self, name: &str) -> Option<(f32, f32)> {
        self.file(name).map(|file| (file.size[0], file.size[1]))
    }

    /// The designer changed a widget that comes from a file. While dragging
    /// the change is only drawn; when it is final the file is written, keeping
    /// whatever the author wrote that still holds.
    pub(super) fn set_file_parts(&mut self, name: &str, parts: Vec<Part>, dragging: bool) {
        let Some(file) = self.file(name).cloned() else { return };
        let parts = lumen_ipc::parts::sanitise_parts(parts);
        if dragging {
            self.files.drafts.insert(name.to_owned(), parts);
            self.draw(None);
            return;
        }
        self.files.drafts.remove(name);
        let folder = self.options.widget_files_dir.clone();
        let path = folder.join(name);
        let previous = std::fs::read_to_string(&path).ok();
        let text = widget_file::render(&file.name, (file.size[0], file.size[1]), &parts, previous.as_deref(), &|other| read_include(&folder, other));
        let temp = path.with_extension("yaml.tmp");
        if let Err(error) = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, &path)) {
            warn!("could not save {}: {error}", path.display());
        }
        // No need to wait for the watcher to say so.
        self.reload_files();
    }
}

fn read_include(folder: &Path, name: &str) -> Option<String> {
    widget_file::is_file_name(name).then(|| std::fs::read_to_string(folder.join(name)).ok()).flatten()
}

/// Reads every `.yaml` file in the folder. A widget file with a mistake keeps
/// the name, size and parts it had in `previous`, with the mistake noted.
fn read_folder(folder: &Path, previous: &[WidgetFileInfo]) -> Vec<WidgetFileInfo> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_file()).collect();
    paths.sort();
    let mut files = Vec::new();
    for path in paths.into_iter().take(200) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()).filter(|name| widget_file::is_file_name(name)).map(str::to_owned) else { continue };
        // Anything large is not a widget someone wrote.
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 512 * 1024) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let blank = || WidgetFileInfo { file: name.clone(), is_widget: true, name: name.trim_end_matches(".yaml").trim_end_matches(".yml").to_owned(), size: [widget_file::DEFAULT_SIZE.0, widget_file::DEFAULT_SIZE.1], parts: Vec::new(), error: None, error_line: None };
        if !widget_file::is_widget(&text) {
            files.push(WidgetFileInfo { is_widget: false, ..blank() });
            continue;
        }
        files.push(match widget_file::parse(&text, &|other| read_include(folder, other)) {
            Ok(file) => WidgetFileInfo { name: file.name, size: [file.size.0, file.size.1], parts: file.parts, ..blank() },
            Err(error) => {
                let last_good = previous.iter().find(|old| old.file == name && old.is_widget).cloned().unwrap_or_else(blank);
                WidgetFileInfo { error: Some(error.message), error_line: error.line, ..last_good }
            }
        });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumen-files-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_is_read_with_widgets_shared_files_and_others_told_apart() {
        let dir = folder("read");
        std::fs::write(dir.join("shared.yaml"), "variables:\n  big: 30\n").unwrap();
        std::fs::write(dir.join("clock.yaml"), "lumen-widget: 1\nname: My clock\ninclude: shared.yaml\nparts:\n  - id: time\n    text: \"{clock.time}\"\n    size: $big\n").unwrap();
        std::fs::write(dir.join("layout.json"), "{}").unwrap();
        std::fs::write(dir.join("has space.yaml"), "lumen-widget: 1\n").unwrap();
        let files = read_folder(&dir, &[]);
        assert_eq!(files.iter().map(|f| (f.file.as_str(), f.is_widget)).collect::<Vec<_>>(), [("clock.yaml", true), ("shared.yaml", false)]);
        assert_eq!((files[0].name.as_str(), files[0].parts[0].size), ("My clock", 30.0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_mistake_keeps_the_last_good_version_and_says_where() {
        let dir = folder("mistake");
        let good = "lumen-widget: 1\nname: Good\nparts:\n  - id: a\n    text: hello\n";
        std::fs::write(dir.join("w.yaml"), good).unwrap();
        let first = read_folder(&dir, &[]);
        std::fs::write(dir.join("w.yaml"), good.replace("text: hello", "color: \"#fff\"")).unwrap();
        let second = read_folder(&dir, &first);
        assert_eq!((second[0].name.as_str(), second[0].parts[0].text.as_str()), ("Good", "hello"), "still showing the old one");
        assert!(second[0].error.as_deref().unwrap().contains("not a theme colour"));
        assert_eq!(second[0].error_line, Some(4));
        std::fs::write(dir.join("w.yaml"), good).unwrap();
        assert_eq!(read_folder(&dir, &second), first, "fixing the file clears the mistake");
        let _ = std::fs::remove_dir_all(dir);
    }
}
