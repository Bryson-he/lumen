//! Widget packages: one `.lumenwidget` file holding a widget file, the files
//! it includes, and the font files it uses, so a widget can be shared whole.
//!
//! A package is a zip:
//! - `package.json`: `{ "lumenWidgetPackage": 1, "name": …, "main": "big-clock.yaml" }`
//! - `widgets/<name>.yaml`: the widget file and each file it includes
//! - `widgets/<name>.luau`: the widget's script, if it has one
//! - `fonts/<file>`: font files Lumen was given that the widget's parts use
//!
//! A script that arrives in a package is never allowed to run by the import:
//! the user is asked about it like any other script.
//!
//! Importing never overwrites anything: a name that is taken gets a new one,
//! and the widget's `include:` line is pointed at the new name.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use lumen_ipc::widget_file;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_ENTRIES: usize = 40;
const MAX_YAML_BYTES: u64 = 512 * 1024;
const MAX_FONT_BYTES: u64 = 40 * 1024 * 1024;
const FONT_EXTENSIONS: [&str; 5] = ["ttf", "otf", "ttc", "woff", "woff2"];

/// What an import added.
#[derive(Debug, PartialEq, Eq)]
pub struct Imported {
    /// The widget file's name in the widgets folder.
    pub file: String,
    /// Whether any font file was added (the widgets then need to load fonts again).
    pub fonts_added: bool,
}

/// Writes `file` (in `widgets_dir`) and everything it needs to `destination`,
/// adding the `.lumenwidget` extension if it is missing. Returns where it was written.
pub fn export(widgets_dir: &Path, fonts_dir: &Path, file: &str, destination: &Path) -> Result<PathBuf, String> {
    let text = fs::read_to_string(widgets_dir.join(file)).map_err(|e| format!("could not read {file}: {e}"))?;
    let read_include = |name: &str| widget_file::is_file_name(name).then(|| fs::read_to_string(widgets_dir.join(name)).ok()).flatten();
    // Only a file that works is worth sharing.
    let widget = widget_file::parse(&text, &read_include).map_err(|e| format!("{file} has a mistake{}: {}", e.line.map_or(String::new(), |line| format!(" on line {line}")), e.message))?;

    let mut path = destination.to_owned();
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lumenwidget")) {
        path.as_mut_os_string().push(".lumenwidget");
    }
    let failed = |e: &dyn std::fmt::Display| format!("could not write {}: {e}", path.display());
    let temp = path.with_extension("lumenwidget.tmp");
    let written = (|| -> zip::result::ZipResult<()> {
        let mut zip = ZipWriter::new(fs::File::create(&temp)?);
        let text_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("package.json", text_options)?;
        zip.write_all(serde_json::json!({ "lumenWidgetPackage": 1, "name": widget.name, "main": file }).to_string().as_bytes())?;
        zip.start_file(format!("widgets/{file}"), text_options)?;
        zip.write_all(text.as_bytes())?;
        for include in widget_file::includes(&text) {
            if let Some(shared) = read_include(&include) {
                zip.start_file(format!("widgets/{include}"), text_options)?;
                zip.write_all(shared.as_bytes())?;
            }
        }
        if let Some(script) = &widget.script
            && let Ok(source) = fs::read_to_string(widgets_dir.join(script))
        {
            zip.start_file(format!("widgets/{script}"), text_options)?;
            zip.write_all(source.as_bytes())?;
        }
        // The font files Lumen was given whose family a part asks for. (Fonts
        // installed in Windows, and the two that ship with Lumen, are not copied.)
        let wanted: Vec<&str> = widget.parts.iter().filter_map(|part| part.font.as_deref()).collect();
        for (name, bytes) in font_files(fonts_dir) {
            if lumen_render::font_families_in(&[&bytes]).iter().any(|family| wanted.contains(&family.as_str())) {
                zip.start_file(format!("fonts/{name}"), SimpleFileOptions::default().compression_method(CompressionMethod::Stored))?;
                zip.write_all(&bytes)?;
            }
        }
        zip.finish()?;
        Ok(())
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temp);
        return Err(failed(&error));
    }
    fs::rename(&temp, &path).map_err(|e| failed(&e))?;
    Ok(path)
}

/// Adds the widget in `package` to `widgets_dir`, and its fonts to `fonts_dir`.
pub fn import(widgets_dir: &Path, fonts_dir: &Path, package: &Path) -> Result<Imported, String> {
    let not_a_package = |why: &dyn std::fmt::Display| format!("{} is not a Lumen widget package ({why})", package.display());
    let mut archive = ZipArchive::new(fs::File::open(package).map_err(|e| not_a_package(&e))?).map_err(|e| not_a_package(&e))?;
    if archive.len() > MAX_ENTRIES {
        return Err(not_a_package(&"it holds too many files"));
    }
    let manifest: serde_json::Value = serde_json::from_slice(&entry(&mut archive, "package.json", MAX_YAML_BYTES).map_err(|e| not_a_package(&e))?).map_err(|e| not_a_package(&e))?;
    let main = manifest.get("main").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned();
    if manifest.get("lumenWidgetPackage").and_then(serde_json::Value::as_u64) != Some(1) || !widget_file::is_file_name(&main) {
        return Err(not_a_package(&"its package.json does not describe one"));
    }
    let mut text = String::from_utf8(entry(&mut archive, &format!("widgets/{main}"), MAX_YAML_BYTES).map_err(|e| not_a_package(&e))?).map_err(|_| not_a_package(&"its widget file is not text"))?;

    fs::create_dir_all(widgets_dir).map_err(|e| format!("could not create {}: {e}", widgets_dir.display()))?;
    let stem = |name: &str| name.trim_end_matches(".yaml").trim_end_matches(".yml").to_owned();
    let free = |wanted: &str| {
        let (mut name, mut number) = (wanted.to_owned(), 2);
        while widgets_dir.join(&name).exists() {
            name = format!("{}-{number}.yaml", stem(wanted));
            number += 1;
        }
        name
    };

    // The files it includes first, so the widget is complete the moment it lands.
    let mut held: Vec<(String, String)> = Vec::new();
    for include in widget_file::includes(&text) {
        if !widget_file::is_file_name(&include) {
            return Err(not_a_package(&"it includes a file outside the widgets folder"));
        }
        let shared = String::from_utf8(entry(&mut archive, &format!("widgets/{include}"), MAX_YAML_BYTES).map_err(|e| not_a_package(&e))?).map_err(|_| not_a_package(&"an included file is not text"))?;
        held.push((include, shared));
    }
    for (include, shared) in &held {
        let existing = fs::read_to_string(widgets_dir.join(include)).ok();
        if existing.as_deref() == Some(shared.as_str()) {
            continue;
        }
        // The name is taken by something different: this one gets a name of its own.
        let name = if existing.is_some() { free(&format!("{}-{include}", stem(&main))) } else { include.clone() };
        fs::write(widgets_dir.join(&name), shared).map_err(|e| format!("could not write {name}: {e}"))?;
        if &name != include {
            text = widget_file::rename_include(&text, include, &name);
        }
    }
    // Check it reads, with its includes as they now are, before it is added.
    let read_include = |name: &str| widget_file::is_file_name(name).then(|| fs::read_to_string(widgets_dir.join(name)).ok()).flatten();
    let widget = widget_file::parse(&text, &read_include).map_err(|e| not_a_package(&format!("its widget file has a mistake: {}", e.message)))?;
    // Its script, under a name that is free (or the same script already there).
    if let Some(script) = &widget.script {
        let source = String::from_utf8(entry(&mut archive, &format!("widgets/{script}"), widget_file::MAX_SCRIPT_BYTES).map_err(|e| not_a_package(&e))?).map_err(|_| not_a_package(&"its script is not text"))?;
        let same = |name: &str| fs::read_to_string(widgets_dir.join(name)).is_ok_and(|there| there == source);
        let (base, mut name, mut number) = (script.trim_end_matches(".luau"), script.clone(), 2);
        while widgets_dir.join(&name).exists() && !same(&name) {
            name = format!("{base}-{number}.luau");
            number += 1;
        }
        if !same(&name) {
            fs::write(widgets_dir.join(&name), &source).map_err(|e| format!("could not write {name}: {e}"))?;
        }
        if &name != script {
            text = widget_file::rename_script(&text, &name);
        }
    }
    let file = free(&main);
    fs::write(widgets_dir.join(&file), &text).map_err(|e| format!("could not write {file}: {e}"))?;

    // Fonts: only real font files, under plain names, and never over one already there.
    let mut fonts_added = false;
    let names: Vec<String> = archive.file_names().flatten().filter_map(|name| name.strip_prefix("fonts/").map(str::to_owned)).collect();
    for name in names {
        let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c)) && !name.contains("..");
        let is_font = Path::new(&name).extension().and_then(|ext| ext.to_str()).is_some_and(|ext| FONT_EXTENSIONS.contains(&ext.to_lowercase().as_str()));
        if !plain || !is_font || fonts_dir.join(&name).exists() {
            continue;
        }
        let Ok(bytes) = entry(&mut archive, &format!("fonts/{name}"), MAX_FONT_BYTES) else { continue };
        if lumen_render::font_families_in(&[&bytes]).is_empty() {
            continue;
        }
        if fs::create_dir_all(fonts_dir).and_then(|()| fs::write(fonts_dir.join(&name), bytes)).is_ok() {
            fonts_added = true;
        }
    }
    Ok(Imported { file, fonts_added })
}

fn entry(archive: &mut ZipArchive<fs::File>, name: &str, limit: u64) -> Result<Vec<u8>, String> {
    let entry = archive.by_name(name).map_err(|_| format!("it has no {name}"))?;
    if entry.size() > limit {
        return Err(format!("{name} is too large"));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    // The size in the header can lie; never read past the limit.
    entry.take(limit + 1).read_to_end(&mut bytes).map_err(|e| format!("{name} could not be read: {e}"))?;
    if bytes.len() as u64 > limit { Err(format!("{name} is too large")) } else { Ok(bytes) }
}

fn font_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries.flatten().filter_map(|item| Some((item.file_name().to_str()?.to_owned(), fs::read(item.path()).ok()?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("lumen-package-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (widgets, fonts) = (root.join("widgets"), root.join("fonts"));
        fs::create_dir_all(&widgets).unwrap();
        fs::create_dir_all(&fonts).unwrap();
        (root, widgets, fonts)
    }

    const SHARED: &str = "variables:\n  big: 44\n";
    const WIDGET: &str = "lumen-widget: 1\nname: Shared clock\ninclude: shared.yaml\nparts:\n  - id: time\n    text: \"{clock.time}\"\n    size: $big\n    font: Space Grotesk\n";

    #[test]
    fn a_package_carries_the_widget_its_includes_and_its_fonts() {
        let (root, widgets, fonts) = dirs("round-trip");
        fs::write(widgets.join("shared.yaml"), SHARED).unwrap();
        fs::write(widgets.join("clock.yaml"), WIDGET).unwrap();
        // A font file the user added whose family the widget asks for, and one it does not use.
        fs::write(fonts.join("grotesk.woff2"), lumen_widgets::BUNDLED_FONTS[3]).unwrap();
        fs::write(fonts.join("inter.woff2"), lumen_widgets::BUNDLED_FONTS[0]).unwrap();
        let package = export(&widgets, &fonts, "clock.yaml", &root.join("shared-clock")).unwrap();
        assert!(package.ends_with("shared-clock.lumenwidget"));
        let mut names: Vec<String> = ZipArchive::new(fs::File::open(&package).unwrap()).unwrap().file_names().flatten().map(|name| name.into_owned()).collect();
        names.sort();
        assert_eq!(names, ["fonts/grotesk.woff2", "package.json", "widgets/clock.yaml", "widgets/shared.yaml"], "only the font it uses");

        // On another PC: nothing there yet.
        let (_, other_widgets, other_fonts) = dirs("round-trip-other");
        let imported = import(&other_widgets, &other_fonts, &package).unwrap();
        assert_eq!(imported, Imported { file: "clock.yaml".into(), fonts_added: true });
        assert_eq!(fs::read_to_string(other_widgets.join("clock.yaml")).unwrap(), WIDGET);
        assert_eq!(fs::read_to_string(other_widgets.join("shared.yaml")).unwrap(), SHARED);
        assert!(other_fonts.join("grotesk.woff2").is_file());

        // Imported again: a second copy under a new name; the identical shared file and font are reused.
        let again = import(&other_widgets, &other_fonts, &package).unwrap();
        assert_eq!(again, Imported { file: "clock-2.yaml".into(), fonts_added: false });
        assert_eq!(fs::read_dir(&other_widgets).unwrap().count(), 3);
    }

    #[test]
    fn a_shared_file_that_differs_is_never_overwritten() {
        let (root, widgets, fonts) = dirs("clash");
        fs::write(widgets.join("shared.yaml"), SHARED).unwrap();
        fs::write(widgets.join("clock.yaml"), WIDGET).unwrap();
        let package = export(&widgets, &fonts, "clock.yaml", &root.join("clock.lumenwidget")).unwrap();

        let (_, mine, my_fonts) = dirs("clash-mine");
        fs::write(mine.join("shared.yaml"), "variables:\n  big: 12\n").unwrap();
        let imported = import(&mine, &my_fonts, &package).unwrap();
        assert_eq!(fs::read_to_string(mine.join("shared.yaml")).unwrap(), "variables:\n  big: 12\n", "mine is untouched");
        assert_eq!(fs::read_to_string(mine.join("clock-shared.yaml")).unwrap(), SHARED);
        let text = fs::read_to_string(mine.join(&imported.file)).unwrap();
        assert!(text.contains("include: clock-shared.yaml"), "{text}");
        let read = |name: &str| fs::read_to_string(mine.join(name)).ok();
        assert_eq!(widget_file::parse(&text, &read).unwrap().parts[0].size, 44.0, "and it still looks as its author made it");
    }

    #[test]
    fn things_that_are_not_packages_are_refused() {
        let (root, widgets, fonts) = dirs("refuse");
        let junk = root.join("junk.lumenwidget");
        fs::write(&junk, "not a zip").unwrap();
        assert!(import(&widgets, &fonts, &junk).unwrap_err().contains("not a Lumen widget package"));

        let make = |name: &str, entries: &[(&str, &str)]| {
            let path = root.join(name);
            let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
            for (entry, body) in entries {
                zip.start_file(*entry, SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
            path
        };
        let escaping = make("escape.lumenwidget", &[("package.json", r#"{"lumenWidgetPackage":1,"main":"../evil.yaml"}"#)]);
        assert!(import(&widgets, &fonts, &escaping).is_err(), "a name that leaves the folder");
        let broken = make("broken.lumenwidget", &[("package.json", r#"{"lumenWidgetPackage":1,"main":"w.yaml"}"#), ("widgets/w.yaml", "lumen-widget: 1\nparts:\n  - id: a\n    color: \"#fff\"\n")]);
        assert!(import(&widgets, &fonts, &broken).unwrap_err().contains("mistake"));
        let fake_font = make("font.lumenwidget", &[("package.json", r#"{"lumenWidgetPackage":1,"main":"w.yaml"}"#), ("widgets/w.yaml", "lumen-widget: 1\n"), ("fonts/evil.exe", "MZ"), ("fonts/notfont.ttf", "hello")]);
        let imported = import(&widgets, &fonts, &fake_font).unwrap();
        assert!(!imported.fonts_added && fs::read_dir(&fonts).unwrap().count() == 0, "only real font files are kept");
        assert_eq!(fs::read_dir(&widgets).unwrap().count(), 1, "nothing from the refused packages was written");
        assert!(export(&widgets, &fonts, "missing.yaml", &root.join("x")).is_err());
    }

    #[test]
    fn a_package_carries_the_widgets_script_and_never_replaces_one() {
        let root = std::env::temp_dir().join(format!("lumen-package-script-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (from, to, fonts) = (root.join("from"), root.join("to"), root.join("fonts"));
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("dice.yaml"), "lumen-widget: 1\nname: Dice\nscript: dice.luau\npermissions: [files]\nparts:\n  - id: roll\n    text: \"{script.roll}\"\n").unwrap();
        fs::write(from.join("dice.luau"), "function update() return { roll = 4 } end").unwrap();
        let package = export(&from, &fonts, "dice.yaml", &root.join("dice")).unwrap();

        // Someone else's script of the same name is already here.
        fs::write(to.join("dice.luau"), "function update() return { roll = 1 } end").unwrap();
        let imported = import(&to, &fonts, &package).unwrap();
        assert_eq!(fs::read_to_string(to.join("dice.luau")).unwrap(), "function update() return { roll = 1 } end", "left alone");
        assert_eq!(fs::read_to_string(to.join("dice-2.luau")).unwrap(), "function update() return { roll = 4 } end");
        let widget = widget_file::parse(&fs::read_to_string(to.join(&imported.file)).unwrap(), &|_| None).unwrap();
        assert_eq!((widget.script.as_deref(), widget.permissions.files), (Some("dice-2.luau"), true));

        // The same script again is reused, not copied.
        let again = import(&to, &fonts, &package).unwrap();
        assert_eq!(again.file, "dice-2.yaml");
        assert!(!to.join("dice-3.luau").exists());
        let _ = fs::remove_dir_all(root);
    }

}
