//! A theme written out for other apps: a terminal colour scheme, colour
//! settings for VS Code, CSS variables and plain JSON.
//!
//! Terminals want sixteen named colours (red, green, …) that a theme's eight
//! roles do not have. They are made here: each at its usual hue, pulled a
//! little towards the theme's primary so the set belongs with the theme, and
//! light or dark enough to read on the theme's surface.

use material_colors::{blend::harmonize, color::Rgb, hct::Hct};
use serde_json::{Map, Value, json};

use crate::{Color, Mode, Theme};

/// Text in a terminal should be at least this readable against the background (WCAG AA).
pub const TERMINAL_CONTRAST: f64 = 4.5;

/// The sixteen colours a terminal names, in the usual order: black, red,
/// green, yellow, blue, purple, cyan, white, then the bright eight.
pub const ANSI_NAMES: [&str; 16] = ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightPurple", "brightCyan", "brightWhite"];

/// Hues (in HCT degrees) of red, green, yellow, blue, purple and cyan.
const HUES: [f64; 6] = [25.0, 145.0, 95.0, 265.0, 310.0, 200.0];

fn with_tone(color: Color, tone: f64) -> Color {
    let mut hct = Hct::new(color.into());
    hct.set_tone(tone.clamp(0.0, 100.0));
    Rgb::from(hct).into()
}

/// Moves a colour's tone away from the background, a step at a time, until it reads against it.
fn readable(color: Color, background: Color, dark: bool, wanted: f64) -> Color {
    let mut tone = Hct::new(color.into()).get_tone();
    let mut current = color;
    while current.contrast_ratio(background) < wanted && (0.0..=100.0).contains(&tone) {
        tone += if dark { 2.0 } else { -2.0 };
        current = with_tone(color, tone);
    }
    current
}

/// The sixteen terminal colours for a theme, in the order of [`ANSI_NAMES`].
pub fn ansi(theme: &Theme) -> [Color; 16] {
    let dark = theme.mode == Mode::Dark;
    let roles = &theme.roles;
    let hued = |hue: f64, bright: bool| {
        let tone = match (dark, bright) {
            (true, false) => 70.0,
            (true, true) => 82.0,
            (false, false) => 42.0,
            (false, true) => 32.0,
        };
        let plain: Rgb = Hct::from(hue, 52.0, tone).into();
        let themed: Color = harmonize(plain, roles.primary.into()).into();
        readable(themed, roles.surface, dark, TERMINAL_CONTRAST)
    };
    // The greys carry the surface's own tint.
    let grey = |tone: f64| with_tone(roles.neutral, tone);
    let (black, bright_black, white, bright_white) = if dark {
        (grey(22.0), readable(grey(55.0), roles.surface, true, 3.0), grey(86.0), grey(97.0))
    } else {
        (grey(18.0), readable(grey(48.0), roles.surface, false, 3.0), grey(72.0), grey(88.0))
    };
    let mut colors = [black; 16];
    colors[7] = white;
    colors[8] = bright_black;
    colors[15] = bright_white;
    for (index, hue) in HUES.into_iter().enumerate() {
        colors[index + 1] = hued(hue, false);
        colors[index + 9] = hued(hue, true);
    }
    colors
}

/// The colour behind selected text: the primary, dimmed towards the surface.
fn selection(theme: &Theme) -> Color {
    with_tone(theme.roles.primary, if theme.mode == Mode::Dark { 30.0 } else { 85.0 })
}

/// Black or white, whichever reads better on `color`.
fn on(color: Color) -> Color {
    let (black, white) = (Color::new(0, 0, 0), Color::new(255, 255, 255));
    if color.contrast_ratio(black) >= color.contrast_ratio(white) { black } else { white }
}

/// A Windows Terminal colour scheme called `name`.
pub fn terminal_scheme(theme: &Theme, name: &str) -> Value {
    let mut scheme = Map::new();
    scheme.insert("name".into(), json!(name));
    scheme.insert("background".into(), json!(theme.roles.surface));
    scheme.insert("foreground".into(), json!(theme.roles.on_surface));
    scheme.insert("cursorColor".into(), json!(theme.roles.primary));
    scheme.insert("selectionBackground".into(), json!(selection(theme)));
    for (name, color) in ANSI_NAMES.iter().zip(ansi(theme)) {
        scheme.insert((*name).into(), json!(color));
    }
    Value::Object(scheme)
}

/// A Windows Terminal fragment holding the scheme: dropped into Terminal's
/// fragments folder, it adds the scheme without touching the user's settings.
pub fn terminal_fragment(theme: &Theme, name: &str) -> Value {
    json!({ "schemes": [terminal_scheme(theme, name)] })
}

/// Colour settings for VS Code: the contents of `workbench.colorCustomizations`, ready to paste into its settings.
pub fn vscode(theme: &Theme) -> Value {
    let roles = &theme.roles;
    let colors = ansi(theme);
    let mut map = Map::new();
    let mut set = |key: &str, color: Color| {
        map.insert(key.to_owned(), json!(color));
    };
    for key in ["editor.background", "terminal.background", "panel.background", "tab.activeBackground", "editorGroupHeader.tabsBackground"] {
        set(key, roles.surface);
    }
    for key in ["sideBar.background", "activityBar.background", "titleBar.activeBackground", "statusBar.background", "tab.inactiveBackground", "editorWidget.background", "input.background", "dropdown.background"] {
        set(key, roles.neutral);
    }
    for key in ["editor.foreground", "terminal.foreground", "foreground", "sideBar.foreground", "activityBar.foreground", "titleBar.activeForeground", "statusBar.foreground", "tab.activeForeground", "input.foreground"] {
        set(key, roles.on_surface);
    }
    for key in ["panel.border", "sideBar.border", "tab.border", "editorGroup.border", "input.border", "editorLineNumber.foreground", "tab.inactiveForeground", "descriptionForeground"] {
        set(key, roles.outline);
    }
    for key in ["focusBorder", "editorCursor.foreground", "terminalCursor.foreground", "button.background", "activityBarBadge.background", "progressBar.background", "tab.activeBorderTop", "textLink.foreground", "editorLineNumber.activeForeground"] {
        set(key, roles.primary);
    }
    set("button.foreground", on(roles.primary));
    set("activityBarBadge.foreground", on(roles.primary));
    set("editor.selectionBackground", selection(theme));
    set("list.activeSelectionBackground", selection(theme));
    set("list.activeSelectionForeground", roles.on_surface);
    set("textLink.activeForeground", roles.accent);
    set("editor.findMatchBorder", roles.accent);
    for (name, color) in ANSI_NAMES.iter().zip(colors) {
        // VS Code says "magenta" where Windows Terminal says "purple".
        let name = name.replace("urple", "agenta").replace("pagenta", "magenta").replace("Pagenta", "Magenta");
        let key = format!("terminal.ansi{}{}", name[..1].to_uppercase(), &name[1..]);
        set(&key, color);
    }
    json!({ "workbench.colorCustomizations": Value::Object(map) })
}

fn kebab(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// The theme as CSS variables (`--lumen-primary` and so on), for a web page or any app styled with CSS.
pub fn css(theme: &Theme) -> String {
    let roles = &theme.roles;
    let mut out = format!("/* Lumen theme \"{}\" ({}). Written by Lumen each time a theme is kept; changes made here are replaced. */\n:root {{\n", theme.name.replace("*/", ""), theme.mode);
    let named = [("primary", roles.primary), ("secondary", roles.secondary), ("tertiary", roles.tertiary), ("neutral", roles.neutral), ("surface", roles.surface), ("on-surface", roles.on_surface), ("outline", roles.outline), ("accent", roles.accent)];
    for (name, color) in named {
        out.push_str(&format!("  --lumen-{name}: {color};\n"));
    }
    out.push_str(&format!("  --lumen-on-primary: {};\n  --lumen-selection: {};\n", on(roles.primary), selection(theme)));
    for (name, color) in ANSI_NAMES.iter().zip(ansi(theme)) {
        out.push_str(&format!("  --lumen-term-{}: {color};\n", kebab(name)));
    }
    out.push_str(&format!("  --lumen-radius: {}px;\n  --lumen-border: {}px;\n  --lumen-font: \"{}\";\n", theme.style.radius, theme.style.border, theme.style.font.replace(['"', '\\', ';'], "")));
    out.push_str("  color-scheme: ");
    out.push_str(if theme.mode == Mode::Dark { "dark" } else { "light" });
    out.push_str(";\n}\n");
    out
}

/// The theme as plain JSON: its roles, the terminal colours and its style, for scripts and other tools.
pub fn palette(theme: &Theme) -> Value {
    let terminal: Map<String, Value> = ANSI_NAMES.iter().zip(ansi(theme)).map(|(name, color)| ((*name).to_owned(), json!(color))).collect();
    json!({
        "name": theme.name,
        "mode": theme.mode,
        "roles": theme.roles,
        "onPrimary": on(theme.roles.primary),
        "selection": selection(theme),
        "terminal": terminal,
        "style": theme.style,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Variant;

    fn themes() -> Vec<Theme> {
        let mut all = Vec::new();
        for seed in ["#2F5BEA", "#E53935", "#2E7D32", "#F9A825", "#6A1B9A", "#808080", "#00ACC1"] {
            for mode in [Mode::Dark, Mode::Light] {
                for variant in [Variant::Tonal, Variant::Vibrant, Variant::Expressive, Variant::Monochrome] {
                    all.push(Theme::from_color(seed.parse().unwrap(), variant, mode, 0).unwrap());
                }
            }
        }
        all
    }

    #[test]
    fn every_terminal_colour_reads_on_the_background() {
        for theme in themes() {
            let colors = ansi(&theme);
            for (name, color) in ANSI_NAMES.iter().zip(colors) {
                // Black on a dark theme (and white on a light one) is meant to sit close to the background.
                let wanted = match *name {
                    "black" | "white" | "brightWhite" => continue,
                    "brightBlack" => 3.0,
                    _ => TERMINAL_CONTRAST,
                };
                let ratio = color.contrast_ratio(theme.roles.surface);
                assert!(ratio >= wanted, "{name} {color} on {} in {:?}/{:?}: {ratio:.2}", theme.roles.surface, theme.mode, theme.variant);
            }
            assert!(theme.roles.on_surface.contrast_ratio(selection(&theme)) >= 3.0, "text on the selection in {:?}/{:?}", theme.mode, theme.variant);
        }
    }

    #[test]
    fn the_named_colours_keep_their_names() {
        // Red is still red and blue still blue, whatever the theme's own colour.
        for theme in themes() {
            let colors = ansi(&theme);
            let hue = |index: usize| Hct::new(colors[index].into()).get_hue();
            let near = |index: usize, wanted: f64| {
                let d = (hue(index) - wanted).abs() % 360.0;
                d.min(360.0 - d) <= 40.0
            };
            for (offset, wanted) in HUES.into_iter().enumerate() {
                assert!(near(offset + 1, wanted) && near(offset + 9, wanted), "{} is at hue {:.0}, wanted about {wanted} ({:?})", ANSI_NAMES[offset + 1], hue(offset + 1), theme.mode);
            }
        }
    }

    #[test]
    fn a_terminal_scheme_has_every_key_windows_terminal_reads() {
        let theme = Theme::fallback();
        let fragment = terminal_fragment(&theme, "Lumen");
        let scheme = &fragment["schemes"][0];
        assert_eq!(scheme["name"], "Lumen");
        for key in ["background", "foreground", "cursorColor", "selectionBackground"].into_iter().chain(ANSI_NAMES) {
            let value = scheme[key].as_str().unwrap_or_else(|| panic!("{key} is missing"));
            assert!(value.len() == 7 && value.starts_with('#') && value.parse::<Color>().is_ok(), "{key}: {value}");
        }
        assert_eq!(scheme.as_object().unwrap().len(), 21);
        assert_eq!(scheme["background"], json!(theme.roles.surface));
    }

    #[test]
    fn the_other_files_carry_the_theme() {
        let theme = Theme::fallback();
        let text = css(&theme);
        assert!(text.contains(&format!("--lumen-primary: {};", theme.roles.primary)) && text.contains("--lumen-on-surface:") && text.contains("--lumen-term-bright-black:") && text.trim_end().ends_with('}'));
        let code = vscode(&theme);
        let colors = code["workbench.colorCustomizations"].as_object().unwrap();
        assert_eq!(colors["editor.background"], json!(theme.roles.surface));
        for key in ["terminal.ansiRed", "terminal.ansiMagenta", "terminal.ansiBrightMagenta", "terminal.ansiBrightBlack", "terminal.ansiBrightWhite"] {
            assert!(colors.contains_key(key), "{key}");
        }
        assert!(!colors.keys().any(|key| key.contains("urple")));
        let plain = palette(&theme);
        assert_eq!(plain["roles"]["accent"], json!(theme.roles.accent));
        assert_eq!(plain["terminal"].as_object().unwrap().len(), 16);
    }
}
