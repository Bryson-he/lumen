//! Reading a gallery: its index and its files, from the web or from a folder.
//!
//! A gallery on the web is a `gallery.json` at an `https://` address with the
//! shared files beside it (a GitHub repository's raw files work as they are).
//! A folder on this PC works the same way, which is how a gallery is tried
//! out before it is published.
//!
//! Lumen only goes online for the gallery when the Gallery screen asks.

use std::path::{Path, PathBuf};

use lumen_ipc::gallery::is_safe_path;

/// The gallery Lumen ships pointing at. `None` until the gallery has a home online.
pub const DEFAULT_SOURCE: Option<&str> = None;
const INDEX_FILE: &str = "gallery.json";
const INDEX_BYTES: usize = 1024 * 1024;
/// The largest file an item may be (a preset carries its wallpaper).
pub const ITEM_BYTES: usize = 60 * 1024 * 1024;
const TIMEOUT_MS: i32 = 15_000;

/// Where a gallery is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Folder(PathBuf),
    /// `https://{host}{base}` is the folder the index is in; `base` starts and ends with `/`.
    Web { host: String, base: String },
}

/// Works out where a gallery is from what was typed: an `https://` address
/// (of the folder or of its `gallery.json`), or a folder on this PC.
pub fn source_from(text: &str) -> Result<Source, String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("https://") {
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host_ok = !host.is_empty() && host.contains('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        let folder = path.strip_suffix(INDEX_FILE).unwrap_or(path).trim_end_matches('/');
        if !host_ok || !(folder.is_empty() || is_safe_path(folder)) {
            return Err("that is not a gallery address; it should look like https://example.com/gallery or end in gallery.json".into());
        }
        let base = if folder.is_empty() { "/".to_owned() } else { format!("/{folder}/") };
        return Ok(Source::Web { host: host.to_owned(), base });
    }
    if text.starts_with("http://") {
        return Err("a gallery on the web must use https://".into());
    }
    let path = Path::new(text);
    let folder = if path.file_name().is_some_and(|name| name == INDEX_FILE) { path.parent().unwrap_or(path) } else { path };
    if text.is_empty() || !folder.is_absolute() {
        return Err("give the gallery's web address (https://…) or the full path of a folder on this PC".into());
    }
    Ok(Source::Folder(folder.to_owned()))
}

/// The text of the gallery's index.
pub fn index(source: &Source) -> Result<String, String> {
    String::from_utf8(read(source, INDEX_FILE, INDEX_BYTES)?).map_err(|_| "the gallery's index is not text".into())
}

/// One of the gallery's files, by its path relative to the index.
pub fn file(source: &Source, path: &str) -> Result<Vec<u8>, String> {
    read(source, path, ITEM_BYTES)
}

fn read(source: &Source, path: &str, limit: usize) -> Result<Vec<u8>, String> {
    if !is_safe_path(path) {
        return Err(format!("\"{path}\" is not a file inside the gallery"));
    }
    match source {
        Source::Folder(folder) => {
            let full = folder.join(path);
            let size = std::fs::metadata(&full).map_err(|error| format!("could not read {}: {error}", full.display()))?.len();
            if size > limit as u64 {
                return Err(format!("{path} is too large"));
            }
            std::fs::read(&full).map_err(|error| format!("could not read {}: {error}", full.display()))
        }
        Source::Web { host, base } => lumen_win::http::get_bytes(host, &format!("{base}{path}"), TIMEOUT_MS, limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gallery_can_be_a_web_address_or_a_folder() {
        let web = |host: &str, base: &str| Ok(Source::Web { host: host.into(), base: base.into() });
        assert_eq!(source_from("https://raw.githubusercontent.com/someone/lumen-gallery/main/gallery.json"), web("raw.githubusercontent.com", "/someone/lumen-gallery/main/"));
        assert_eq!(source_from(" https://example.com/gallery/ "), web("example.com", "/gallery/"));
        assert_eq!(source_from("https://example.com"), web("example.com", "/"));
        assert_eq!(source_from("C:\\Shared\\gallery"), Ok(Source::Folder("C:\\Shared\\gallery".into())));
        assert_eq!(source_from("C:\\Shared\\gallery\\gallery.json"), Ok(Source::Folder("C:\\Shared\\gallery".into())));
        for bad in ["", "gallery", "http://example.com/gallery", "https://", "https://localhost/x", "https://example.com/../secret", "https://example.com/a b", "https://user@example.com/x", "ftp://example.com/x"] {
            assert!(source_from(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn files_are_only_read_from_inside_the_gallery() {
        let dir = std::env::temp_dir().join(format!("lumen-gallery-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("gallery").join("widgets")).unwrap();
        std::fs::write(dir.join("secret.txt"), "not for sharing").unwrap();
        std::fs::write(dir.join("gallery").join(INDEX_FILE), "{ \"lumenGallery\": 1, \"items\": [] }").unwrap();
        std::fs::write(dir.join("gallery").join("widgets").join("a.lumenwidget"), [1u8, 2, 3]).unwrap();
        let source = Source::Folder(dir.join("gallery"));
        assert!(index(&source).unwrap().contains("lumenGallery"));
        assert_eq!(file(&source, "widgets/a.lumenwidget").unwrap(), [1, 2, 3]);
        for outside in ["../secret.txt", "widgets/../../secret.txt", "/secret.txt"] {
            assert!(file(&source, outside).unwrap_err().contains("not a file inside the gallery"), "{outside}");
        }
        assert!(file(&source, "widgets/missing.lumenwidget").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
