//! Which font families there are to draw with: the ones installed in
//! Windows, and the ones inside given font files. Needs no graphics device.

use windows::{
    Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory, IDWriteFactory5, IDWriteFontCollection},
    core::{Interface, w},
};

/// The font families installed in Windows, sorted by name.
pub fn installed_font_families() -> Vec<String> {
    unsafe {
        let Ok(factory) = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) else { return Vec::new() };
        let mut collection = None;
        if factory.GetSystemFontCollection(&mut collection, false).is_err() {
            return Vec::new();
        }
        collection.map(|collection| family_names(&collection)).unwrap_or_default()
    }
}

/// The font families inside font files (TTF, OTF, WOFF or WOFF2), sorted by
/// name. Data that is not a font contributes nothing.
pub fn font_families_in(files: &[&[u8]]) -> Vec<String> {
    unsafe {
        let Ok(factory) = DWriteCreateFactory::<IDWriteFactory5>(DWRITE_FACTORY_TYPE_SHARED) else { return Vec::new() };
        let Ok((collection, loader)) = crate::gpu::load_fonts(&factory, files) else { return Vec::new() };
        let names = collection.cast::<IDWriteFontCollection>().map(|collection| family_names(&collection)).unwrap_or_default();
        let _ = factory.UnregisterFontFileLoader(&loader);
        names
    }
}

/// Words a font file may tack onto its family name when it holds one weight.
const WEIGHT_WORDS: [&str; 12] = ["Thin", "ExtraLight", "Extra Light", "Light", "Regular", "Medium", "SemiBold", "Semi Bold", "Bold", "ExtraBold", "Extra Bold", "Black"];

/// "Space Grotesk Light" → "Space Grotesk"; `None` if the name ends in no weight word.
fn without_weight(name: &str) -> Option<&str> {
    WEIGHT_WORDS.iter().find_map(|word| name.strip_suffix(word)?.strip_suffix(' ')).filter(|rest| !rest.is_empty())
}

/// The names under which a collection holds the family `plain`, when its
/// files name the family after a weight.
pub(crate) fn file_names_for(collection: &IDWriteFontCollection, plain: &str) -> Vec<String> {
    unsafe { raw_family_names(collection) }.into_iter().filter(|name| without_weight(name) == Some(plain)).collect()
}

/// Family names as people know them: a family named after one of its weights
/// is listed by its plain name.
unsafe fn family_names(collection: &IDWriteFontCollection) -> Vec<String> {
    let mut names: Vec<String> = unsafe { raw_family_names(collection) }.into_iter().map(|name| without_weight(&name).map_or(name.clone(), str::to_owned)).collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

unsafe fn raw_family_names(collection: &IDWriteFontCollection) -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        for index in 0..collection.GetFontFamilyCount() {
            let Ok(strings) = collection.GetFontFamily(index).and_then(|family| family.GetFamilyNames()) else { continue };
            // The English name if there is one, else whichever comes first.
            let (mut at, mut exists) = (0u32, false.into());
            let _ = strings.FindLocaleName(w!("en-us"), &mut at, &mut exists);
            if !exists.as_bool() {
                at = 0;
            }
            let Ok(length) = strings.GetStringLength(at) else { continue };
            let mut buffer = vec![0u16; length as usize + 1];
            if strings.GetString(at, &mut buffer).is_ok() {
                names.push(String::from_utf16_lossy(&buffer[..length as usize]));
            }
        }
    }
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_has_fonts_installed() {
        let families = installed_font_families();
        assert!(families.iter().any(|family| family == "Segoe UI"), "{} families, no Segoe UI", families.len());
        assert!(families.windows(2).all(|pair| pair[0].to_lowercase() <= pair[1].to_lowercase()), "sorted");
    }

    #[test]
    fn font_files_say_which_families_they_hold() {
        let inter: &[u8] = include_bytes!("../../../assets/fonts/inter-latin-400-normal.woff2");
        let grotesk: &[u8] = include_bytes!("../../../assets/fonts/space-grotesk-latin-500-normal.woff2");
        assert_eq!(font_families_in(&[inter, grotesk]), ["Inter", "Space Grotesk"]);
        assert!(font_families_in(&[b"this is not a font"]).is_empty());
        assert!(font_families_in(&[]).is_empty());
    }

    #[test]
    fn a_family_named_after_one_weight_is_known_by_its_plain_name() {
        assert_eq!(without_weight("Space Grotesk Light"), Some("Space Grotesk"));
        assert_eq!(without_weight("Roboto Semi Bold"), Some("Roboto"));
        assert_eq!(without_weight("Inter"), None);
        assert_eq!(without_weight("Light"), None, "a family simply called Light keeps its name");
        assert_eq!(without_weight("Highlight"), None, "only a whole word counts");
    }
}
