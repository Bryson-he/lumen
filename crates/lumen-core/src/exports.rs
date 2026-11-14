//! The kept theme, written out for other apps to use.
//!
//! Every time a theme is kept, Lumen rewrites a handful of ready-made files
//! in its own folder (`exports/`): a Windows Terminal colour scheme, colour
//! settings for VS Code, CSS variables and plain JSON. Lumen never edits
//! another app's settings; the files are there to be pasted or pointed at.
//!
//! The one thing Lumen can switch on by itself is Windows Terminal's own
//! drop-in folder ("fragments"): a file there adds a colour scheme called
//! "Lumen" to Terminal's list without touching Terminal's settings file.
//! Switching it off removes that one file again.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use lumen_theme::{Theme, export};

/// The name of the colour scheme, as it appears in Windows Terminal.
pub const SCHEME_NAME: &str = "Lumen";
/// The files written to the exports folder.
pub const FILES: [&str; 5] = ["windows-terminal.json", "vscode-settings.json", "lumen.css", "lumen.json", "README.txt"];
/// Lumen's own folder and file inside Terminal's fragments folder.
const FRAGMENT_FOLDER: &str = "Lumen";
const FRAGMENT_FILE: &str = "lumen.json";

const README: &str = "These files hold the colours of the theme Lumen is showing now.\r\n\
Lumen rewrites them every time you keep a theme, so do not edit them here: copy what you need.\r\n\
\r\n\
windows-terminal.json\r\n\
  A colour scheme for Windows Terminal. Easiest: in Lumen, open Theme & colours and switch on\r\n\
  \"Add a Lumen colour scheme to Windows Terminal\"; then pick \"Lumen\" in Terminal under\r\n\
  Settings > a profile > Appearance > Colour scheme. It then follows your theme by itself.\r\n\
  By hand: paste this file's contents into the \"schemes\" list of Terminal's settings.json.\r\n\
\r\n\
vscode-settings.json\r\n\
  Colours for VS Code. Open VS Code's settings.json (Ctrl+Shift+P, \"Open User Settings (JSON)\")\r\n\
  and paste the \"workbench.colorCustomizations\" block in. Paste again after changing theme.\r\n\
\r\n\
lumen.css\r\n\
  The colours as CSS variables (--lumen-primary and so on), for a web page, a browser start\r\n\
  page, or any app styled with CSS.\r\n\
\r\n\
lumen.json\r\n\
  The same colours as plain JSON, for scripts and other tools.\r\n";

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default() + "\n"
}

/// Writes through a temporary file, so another app never reads half a file.
fn write_file(path: &Path, text: &str) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    fs::write(&temp, text).and_then(|()| fs::rename(&temp, path))
}

/// Writes every export file for `theme` into `dir`.
pub fn write(dir: &Path, theme: &Theme) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    write_file(&dir.join(FILES[0]), &pretty(&export::terminal_scheme(theme, SCHEME_NAME)))?;
    write_file(&dir.join(FILES[1]), &pretty(&export::vscode(theme)))?;
    write_file(&dir.join(FILES[2]), &export::css(theme))?;
    write_file(&dir.join(FILES[3]), &pretty(&export::palette(theme)))?;
    write_file(&dir.join(FILES[4]), README)
}

/// Windows Terminal's fragments folder for this user: `LUMEN_TERMINAL_FRAGMENTS_DIR`
/// if set (tests), else `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments`.
pub fn terminal_fragments_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LUMEN_TERMINAL_FRAGMENTS_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").filter(|dir| !dir.is_empty())?);
    Some(local.join("Microsoft").join("Windows Terminal").join("Fragments"))
}

/// Whether Windows Terminal looks to be installed for this user.
pub fn terminal_installed() -> bool {
    if std::env::var_os("LUMEN_TERMINAL_FRAGMENTS_DIR").is_some_and(|dir| !dir.is_empty()) {
        return true;
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else { return false };
    let packaged = fs::read_dir(local.join("Packages")).is_ok_and(|entries| entries.flatten().any(|entry| entry.file_name().to_string_lossy().starts_with("Microsoft.WindowsTerminal")));
    packaged || local.join("Microsoft").join("WindowsApps").join("wt.exe").exists() || local.join("Microsoft").join("Windows Terminal").exists()
}

/// Where Lumen's fragment goes inside the fragments folder.
pub fn fragment_path(fragments: &Path) -> PathBuf {
    fragments.join(FRAGMENT_FOLDER).join(FRAGMENT_FILE)
}

/// Adds (or refreshes) the "Lumen" colour scheme in Windows Terminal.
pub fn write_fragment(fragments: &Path, theme: &Theme) -> io::Result<()> {
    let path = fragment_path(fragments);
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    write_file(&path, &pretty(&export::terminal_fragment(theme, SCHEME_NAME)))
}

/// Takes the "Lumen" colour scheme out of Windows Terminal again: removes
/// Lumen's one file, and its folder if nothing else is in it.
pub fn remove_fragment(fragments: &Path) -> io::Result<()> {
    let path = fragment_path(fragments);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(folder) = path.parent() {
        // Fails, harmlessly, if anything else is in there.
        let _ = fs::remove_dir(folder);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumen-exports-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn every_file_is_written_and_follows_the_theme() {
        let dir = folder("write");
        let blue = Theme::from_color("#2F5BEA".parse().unwrap(), lumen_theme::Variant::Tonal, lumen_theme::Mode::Dark, 0).unwrap();
        write(&dir, &blue).unwrap();
        for file in FILES {
            assert!(fs::metadata(dir.join(file)).is_ok_and(|meta| meta.len() > 100), "{file}");
        }
        let scheme: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("windows-terminal.json")).unwrap()).unwrap();
        assert_eq!((scheme["name"].as_str(), scheme["background"].as_str()), (Some("Lumen"), Some(blue.roles.surface.to_string().as_str())));

        let red = Theme::from_color("#E53935".parse().unwrap(), lumen_theme::Variant::Tonal, lumen_theme::Mode::Light, 0).unwrap();
        write(&dir, &red).unwrap();
        let css = fs::read_to_string(dir.join("lumen.css")).unwrap();
        assert!(css.contains(&format!("--lumen-primary: {};", red.roles.primary)) && css.contains("color-scheme: light"));
        assert!(!fs::read_dir(&dir).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")), "no half-written files left");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_terminal_fragment_is_one_file_that_can_be_taken_away_again() {
        let fragments = folder("fragment");
        // Another app's fragment, which must never be touched.
        fs::create_dir_all(fragments.join("OtherApp")).unwrap();
        fs::write(fragments.join("OtherApp").join("theirs.json"), "{}").unwrap();

        write_fragment(&fragments, &Theme::fallback()).unwrap();
        let fragment: serde_json::Value = serde_json::from_str(&fs::read_to_string(fragments.join("Lumen").join("lumen.json")).unwrap()).unwrap();
        assert_eq!(fragment["schemes"][0]["name"], "Lumen");
        assert_eq!(fragment.as_object().unwrap().len(), 1, "a scheme and nothing else: no profiles, no actions");

        remove_fragment(&fragments).unwrap();
        assert!(!fragments.join("Lumen").exists());
        assert!(fragments.join("OtherApp").join("theirs.json").exists());
        remove_fragment(&fragments).unwrap();

        // Something else in Lumen's folder is left alone.
        write_fragment(&fragments, &Theme::fallback()).unwrap();
        fs::write(fragments.join("Lumen").join("notes.txt"), "mine").unwrap();
        remove_fragment(&fragments).unwrap();
        assert!(fragments.join("Lumen").join("notes.txt").exists() && !fragments.join("Lumen").join("lumen.json").exists());
        let _ = fs::remove_dir_all(fragments);
    }
}
