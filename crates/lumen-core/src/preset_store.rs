//! Presets on disk: `presets/<id>.json`, and `.lumen` files for moving a
//! preset between PCs.
//!
//! A `.lumen` file is a zip holding `preset.json` (name, wallpaper per
//! monitor, widget layout, bar config), `theme.json`, and the wallpaper
//! images under `wallpapers/`. It is read defensively: it may come from a
//! stranger, so sizes are capped, only the entries the manifest names are
//! read, and nothing in it chooses where a file is written.

use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

use lumen_ipc::{
    Preset, PresetInfo, PresetManifest, PresetWallpaper,
    presets::{PRESET_EXTENSION, PRESET_FORMAT},
};
use lumen_theme::Theme;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::store::{Store, StoreError, now_secs, slug, unique};

type Result<T> = std::result::Result<T, StoreError>;

/// Limits for `.lumen` files.
const MAX_ENTRIES: usize = 40;
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WALLPAPER_BYTES: u64 = 150 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 80;

impl Store {
    /// Ids are file names; never let one walk out of the presets folder.
    fn check_preset_id(id: &str) -> Result<()> {
        if id.is_empty() || id.contains(['/', '\\', ':']) || id.contains("..") {
            return Err(StoreError::Invalid(format!("\"{id}\" is not a preset id")));
        }
        Ok(())
    }

    pub fn preset(&self, id: &str) -> Result<Preset> {
        Self::check_preset_id(id)?;
        self.read_json(&format!("presets/{id}.json")).map_err(|error| match error {
            StoreError::NotFound(_) => StoreError::NotFound(format!("there is no preset \"{id}\"")),
            other => other,
        })
    }

    /// Every saved preset, most recently used first (then newest first).
    pub fn presets(&self) -> Vec<(String, Preset)> {
        let Ok(entries) = fs::read_dir(self.root().join("presets")) else { return Vec::new() };
        let mut presets: Vec<(String, Preset)> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_owned();
                // A file that does not parse is skipped, not fatal.
                self.preset(&id).ok().map(|preset| (id, preset))
            })
            .collect();
        presets.sort_by(|(a_id, a), (b_id, b)| (b.last_used_at, b.created_at).cmp(&(a.last_used_at, a.created_at)).then_with(|| a_id.cmp(b_id)));
        presets
    }

    pub fn save_preset(&self, id: &str, preset: &Preset) -> Result<()> {
        Self::check_preset_id(id)?;
        self.write_json(&format!("presets/{id}.json"), preset)
    }

    /// A file-safe id for a new preset with this name, not used yet.
    pub fn new_preset_id(&self, name: &str) -> String {
        unique(&slug(name), |candidate| self.root().join(format!("presets/{candidate}.json")).exists())
    }

    pub fn delete_preset(&self, id: &str) -> Result<()> {
        Self::check_preset_id(id)?;
        fs::remove_file(self.root().join(format!("presets/{id}.json"))).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StoreError::NotFound(format!("there is no preset \"{id}\"")),
            _ => StoreError::Io(format!("could not delete the preset: {e}")),
        })?;
        if self.active_preset().as_deref() == Some(id) {
            self.set_active_preset(None)?;
        }
        Ok(())
    }

    /// The list entry for a preset.
    pub fn preset_info(&self, id: &str, preset: &Preset, active: bool) -> PresetInfo {
        let roles = &preset.theme.roles;
        let thumb = preset.wallpapers.first().and_then(|w| self.wallpaper_by_file(&w.file)).map(|entry| entry.thumb).filter(|thumb| Path::new(thumb).is_file());
        PresetInfo {
            id: id.to_owned(),
            name: preset.name.clone(),
            thumb,
            palette: vec![roles.primary, roles.secondary, roles.tertiary, roles.accent, roles.surface],
            created_at: preset.created_at,
            last_used_at: preset.last_used_at,
            widget_count: preset.widgets.as_ref().map_or(0, |layout| layout.widgets.len()),
            active,
        }
    }

    // ----- .lumen files -----

    /// Writes a preset and everything it needs to one `.lumen` file.
    pub fn export_preset(&self, id: &str, destination: &Path) -> Result<()> {
        let preset = self.preset(id)?;
        let failed = |error: &dyn std::fmt::Display| StoreError::Io(format!("could not write {}: {error}", destination.display()));

        // Wallpapers go in under wallpapers/<file name>; two presets entries
        // that share an image share one copy.
        let mut images: Vec<(String, Vec<u8>)> = Vec::new();
        let mut wallpapers = Vec::new();
        for wallpaper in &preset.wallpapers {
            let source = self.resolve(&wallpaper.file);
            let Some(file_name) = source.file_name().and_then(|n| n.to_str()) else { continue };
            let inside = format!("wallpapers/{file_name}");
            if !images.iter().any(|(name, _)| name == &inside) {
                let bytes = fs::read(&source).map_err(|e| StoreError::NotFound(format!("the preset's wallpaper {} is missing: {e}", source.display())))?;
                images.push((inside.clone(), bytes));
            }
            wallpapers.push(PresetWallpaper { monitor: wallpaper.monitor, file: inside });
        }
        let manifest = PresetManifest { format_version: PRESET_FORMAT, name: preset.name.clone(), theme: "theme.json".to_owned(), wallpapers, widgets: preset.widgets.clone(), bar: preset.bar.clone(), flyouts: preset.flyouts.clone(), launcher: preset.launcher.clone(), styles: preset.styles.clone() };

        // Write beside the destination first, so a failure never leaves a broken file in its place.
        let temp = destination.with_extension(format!("{PRESET_EXTENSION}.tmp"));
        let write = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let mut zip = ZipWriter::new(fs::File::create(&temp)?);
            let text = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            // Images are already compressed; storing them as-is is faster and no larger.
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("preset.json", text)?;
            zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
            zip.start_file("theme.json", text)?;
            zip.write_all(serde_json::to_string_pretty(&preset.theme)?.as_bytes())?;
            for (name, bytes) in &images {
                zip.start_file(name.as_str(), stored)?;
                zip.write_all(bytes)?;
            }
            zip.finish()?;
            Ok(())
        };
        let result = write().map_err(|e| failed(&e)).and_then(|()| fs::rename(&temp, destination).map_err(|e| failed(&e)));
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    /// Reads a `.lumen` file: its wallpapers join the library and the preset
    /// is saved under a new id. Nothing is applied to the desktop.
    pub fn import_preset(&self, source: &Path) -> Result<(String, Preset)> {
        let shown = source.display();
        let not_a_preset = |why: &dyn std::fmt::Display| StoreError::Invalid(format!("{shown} is not a Lumen preset: {why}"));
        let file = fs::File::open(source).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StoreError::NotFound(format!("no such file: {shown}")),
            _ => StoreError::Io(format!("could not open {shown}: {e}")),
        })?;
        let mut archive = ZipArchive::new(file).map_err(|e| not_a_preset(&e))?;
        if archive.len() > MAX_ENTRIES {
            return Err(not_a_preset(&"it contains too many files"));
        }

        let manifest: PresetManifest = read_json_entry(&mut archive, "preset.json").map_err(|e| not_a_preset(&e))?;
        if manifest.format_version > PRESET_FORMAT {
            return Err(StoreError::Invalid(format!("{shown} was made by a newer version of Lumen (format {}); update Lumen to use it", manifest.format_version)));
        }
        let mut theme: Theme = read_json_entry(&mut archive, &manifest.theme).map_err(|e| not_a_preset(&e))?;
        theme.contrast = theme.contrast.clamp(-1, 1);
        theme.style.radius = theme.style.radius.min(24);
        theme.style.border = theme.style.border.min(3);

        // Bring each wallpaper into the library; the same image twice is one copy.
        let mut wallpapers = Vec::new();
        let mut imported: Vec<(String, String)> = Vec::new(); // (name in the archive, library file)
        for wallpaper in manifest.wallpapers.iter().filter(|w| w.monitor > 0).take(16) {
            let library_file = match imported.iter().find(|(inside, _)| inside == &wallpaper.file) {
                Some((_, file)) => file.clone(),
                None => {
                    let inside = Path::new(&wallpaper.file);
                    let stem = inside.file_stem().and_then(|s| s.to_str()).unwrap_or("wallpaper");
                    let extension = inside.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
                    let bytes = read_entry(&mut archive, &wallpaper.file, MAX_WALLPAPER_BYTES).map_err(|e| not_a_preset(&e))?;
                    // The name only seeds the library id (through `slug`); it never becomes a path.
                    let entry = self.import_wallpaper_bytes(stem, &extension, &bytes)?;
                    imported.push((wallpaper.file.clone(), entry.file.clone()));
                    entry.file
                }
            };
            wallpapers.push(PresetWallpaper { monitor: wallpaper.monitor, file: library_file });
        }

        // The theme remembers which wallpaper it was made from; point that at
        // the copy now in this library (its name may differ here).
        if let lumen_theme::Source::Wallpaper { path } = &mut theme.source {
            let wanted = Path::new(path.as_str()).file_name().map(|n| n.to_os_string());
            if let Some((_, library_file)) = imported.iter().find(|(inside, _)| Path::new(inside).file_name().map(|n| n.to_os_string()) == wanted) {
                *path = library_file.clone();
            }
        }

        let name: String = manifest.name.trim().chars().take(MAX_NAME_CHARS).collect();
        let name = if name.is_empty() { "Imported preset".to_owned() } else { name };
        let preset = Preset {
            format_version: PRESET_FORMAT,
            name,
            created_at: now_secs(),
            last_used_at: 0,
            theme,
            wallpapers,
            widgets: manifest.widgets.map(lumen_ipc::Layout::sanitised),
            bar: manifest.bar,
            flyouts: manifest.flyouts,
            launcher: manifest.launcher,
            styles: manifest.styles,
        };
        let id = self.new_preset_id(&preset.name);
        self.save_preset(&id, &preset)?;
        Ok((id, preset))
    }
}

/// Reads one named entry, refusing anything larger than `limit`.
fn read_entry(archive: &mut ZipArchive<fs::File>, name: &str, limit: u64) -> std::result::Result<Vec<u8>, String> {
    let entry = archive.by_name(name).map_err(|_| format!("it has no {name}"))?;
    if entry.size() > limit {
        return Err(format!("{name} is too large"));
    }
    // The declared size can lie; the reader is capped as well.
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(limit + 1).read_to_end(&mut bytes).map_err(|e| format!("{name} could not be read: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{name} is too large"));
    }
    Ok(bytes)
}

fn read_json_entry<T: serde::de::DeserializeOwned>(archive: &mut ZipArchive<fs::File>, name: &str) -> std::result::Result<T, String> {
    let bytes = read_entry(archive, name, MAX_JSON_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{name} is not valid: {e}"))
}

#[cfg(test)]
mod tests {
    use lumen_ipc::{Layout, presets::monitor_ref};
    use lumen_theme::{Color, Mode, Variant};

    use super::*;
    use crate::store::tests::{temp_store, test_image};

    /// A preset with a wallpaper on monitors 1 and 2 and a widget on monitor 2.
    fn sample(store: &Store, name: &str) -> (String, Preset) {
        let sky = store.import_wallpaper(&test_image(store.root(), "sky.png", [40, 80, 200])).unwrap();
        let ember = store.import_wallpaper(&test_image(store.root(), "ember.png", [200, 80, 40])).unwrap();
        let mut theme = Theme::from_color(Color::new(40, 80, 200), Variant::Vibrant, Mode::Dark, 0).unwrap();
        theme.name = name.to_owned();
        let mut widgets = Layout::default();
        widgets.widgets[2].monitor = Some(monitor_ref(2));
        widgets.widgets[0].x = 240.0;
        let preset = Preset {
            format_version: PRESET_FORMAT,
            name: name.to_owned(),
            created_at: 100,
            last_used_at: 0,
            theme,
            wallpapers: vec![PresetWallpaper { monitor: 1, file: sky.file }, PresetWallpaper { monitor: 2, file: ember.file }],
            widgets: Some(widgets),
            bar: serde_json::Value::Null,
            flyouts: serde_json::Value::Null,
            launcher: serde_json::Value::Null,
            styles: serde_json::Value::Null,
        };
        let id = store.new_preset_id(name);
        store.save_preset(&id, &preset).unwrap();
        (id, preset)
    }

    #[test]
    fn presets_are_saved_listed_and_deleted() {
        let store = temp_store();
        let (id, preset) = sample(&store, "Night Sky");
        assert_eq!(id, "night-sky");
        assert_eq!(store.preset(&id).unwrap(), preset);
        assert_eq!(store.new_preset_id("Night Sky"), "night-sky-2", "a second one with the same name gets its own id");

        let info = store.preset_info(&id, &preset, true);
        assert_eq!((info.name.as_str(), info.widget_count, info.active, info.palette.len()), ("Night Sky", 3, true, 5));
        assert!(info.thumb.is_some_and(|thumb| Path::new(&thumb).is_file()), "its first wallpaper's thumbnail");

        store.set_active_preset(Some(&id)).unwrap();
        store.delete_preset(&id).unwrap();
        assert!(store.presets().is_empty());
        assert_eq!(store.active_preset(), None, "deleting the active preset clears the marker");
        assert!(matches!(store.delete_preset(&id), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn the_list_is_most_recently_used_first() {
        let store = temp_store();
        let (old, mut old_preset) = sample(&store, "Old Favourite");
        let (new, _) = sample(&store, "Brand New");
        // Neither has been used and both were made at the same moment: listed by id.
        assert_eq!(store.presets().iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), [new.as_str(), old.as_str()]);
        old_preset.last_used_at = 5000;
        store.save_preset(&old, &old_preset).unwrap();
        assert_eq!(store.presets()[0].0, old, "used most recently");
    }

    #[test]
    fn preset_ids_cannot_leave_the_presets_folder() {
        let store = temp_store();
        for bad in ["", "../config", r"..\config", "a/b", "C:evil"] {
            assert!(matches!(store.preset(bad), Err(StoreError::Invalid(_))), "{bad:?}");
            assert!(matches!(store.delete_preset(bad), Err(StoreError::Invalid(_))), "{bad:?}");
        }
        assert!(matches!(store.preset("missing"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn export_then_import_on_a_clean_profile_gives_the_same_preset() {
        let home = temp_store();
        let (id, original) = sample(&home, "Night Sky");
        let file = home.root().join("night-sky.lumen");
        home.export_preset(&id, &file).unwrap();
        assert!(file.is_file() && !file.with_extension("lumen.tmp").exists());

        // A different, empty profile: nothing of the original exists there.
        let elsewhere = temp_store();
        let (new_id, imported) = elsewhere.import_preset(&file).unwrap();
        assert_eq!(new_id, "night-sky");
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.theme, original.theme, "the theme arrives exactly");
        assert_eq!(imported.widgets, original.widgets, "so does the widget layout, monitor numbers included");
        assert_eq!(imported.wallpapers.iter().map(|w| w.monitor).collect::<Vec<_>>(), [1, 2]);

        // The wallpapers were copied into the new library, byte for byte.
        for (theirs, ours) in imported.wallpapers.iter().zip(&original.wallpapers) {
            let copied = fs::read(elsewhere.resolve(&theirs.file)).unwrap();
            assert_eq!(copied, fs::read(home.resolve(&ours.file)).unwrap());
            assert!(elsewhere.wallpaper_by_file(&theirs.file).is_some(), "and are in the library, with a palette");
        }
        assert_eq!(elsewhere.wallpapers().len(), 2);
        assert_eq!(elsewhere.presets().len(), 1);

        // Importing again makes a second preset but does not duplicate the images.
        let (again, _) = elsewhere.import_preset(&file).unwrap();
        assert_eq!(again, "night-sky-2");
        assert_eq!(elsewhere.wallpapers().len(), 2);
    }

    #[test]
    fn one_image_on_two_monitors_is_stored_once() {
        let store = temp_store();
        let (id, mut preset) = sample(&store, "Twins");
        preset.wallpapers[1].file = preset.wallpapers[0].file.clone();
        store.save_preset(&id, &preset).unwrap();
        let file = store.root().join("twins.lumen");
        store.export_preset(&id, &file).unwrap();

        let archive = ZipArchive::new(fs::File::open(&file).unwrap()).unwrap();
        let names: Vec<String> = archive.file_names().map(|name| name.unwrap().into_owned()).collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("wallpapers/")).count(), 1, "{names:?}");
        assert!(names.iter().any(|n| n == "preset.json") && names.iter().any(|n| n == "theme.json"));
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn files_that_are_not_presets_are_refused_and_leave_nothing_behind() {
        let store = temp_store();
        let dir = store.root().to_owned();
        assert!(matches!(store.import_preset(&dir.join("missing.lumen")), Err(StoreError::NotFound(_))));

        fs::write(dir.join("text.lumen"), "hello").unwrap();
        assert!(matches!(store.import_preset(&dir.join("text.lumen")), Err(StoreError::Invalid(_))));

        write_zip(&dir.join("empty.lumen"), &[("readme.txt", b"hi")]);
        assert!(matches!(store.import_preset(&dir.join("empty.lumen")), Err(StoreError::Invalid(_))));

        let theme = serde_json::to_vec(&Theme::fallback()).unwrap();
        let newer = br#"{"formatVersion":99,"name":"Future","theme":"theme.json"}"#;
        write_zip(&dir.join("newer.lumen"), &[("preset.json", newer), ("theme.json", &theme)]);
        let error = store.import_preset(&dir.join("newer.lumen")).unwrap_err().to_string();
        assert!(error.contains("newer version"), "{error}");

        // A wallpaper entry that is not an image.
        let manifest = br#"{"formatVersion":1,"name":"Trick","theme":"theme.json","wallpapers":[{"monitor":1,"file":"wallpapers/evil.exe"}]}"#;
        write_zip(&dir.join("trick.lumen"), &[("preset.json", manifest), ("theme.json", &theme), ("wallpapers/evil.exe", b"MZ")]);
        assert!(matches!(store.import_preset(&dir.join("trick.lumen")), Err(StoreError::Invalid(_))));

        assert!(store.presets().is_empty());
        assert!(store.wallpapers().is_empty());
    }

    #[test]
    fn hostile_names_inside_a_preset_cannot_escape_the_library() {
        let store = temp_store();
        let dir = store.root().to_owned();
        let image = fs::read(test_image(&dir, "pixel.png", [10, 200, 90])).unwrap();
        let theme = serde_json::to_vec(&Theme::fallback()).unwrap();
        let manifest = br#"{"formatVersion":1,"name":"  ../../Sneaky  ","theme":"theme.json","wallpapers":[{"monitor":1,"file":"../../outside.png"}]}"#;
        write_zip(&dir.join("sneaky.lumen"), &[("preset.json", manifest), ("theme.json", &theme), ("../../outside.png", &image)]);

        match store.import_preset(&dir.join("sneaky.lumen")) {
            // Either refused outright...
            Err(error) => assert!(matches!(error, StoreError::Invalid(_))),
            // ...or accepted with everything kept inside the data folder.
            Ok((id, preset)) => {
                assert!(!id.contains(".."));
                assert!(store.root().join(format!("presets/{id}.json")).is_file());
                for wallpaper in &preset.wallpapers {
                    assert!(wallpaper.file.starts_with("wallpapers/") && !wallpaper.file.contains(".."), "{}", wallpaper.file);
                }
            }
        }
        assert!(!dir.parent().unwrap().join("outside.png").exists());
        assert!(!dir.parent().unwrap().parent().unwrap().join("outside.png").exists());
    }
}
