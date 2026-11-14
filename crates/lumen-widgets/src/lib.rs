//! Lumen's built-in desktop widgets and the host that runs them.
//!
//! - [`paint::Paint`]: the colours and style a widget draws with, taken from the theme.
//! - [`clock`], [`stats`], [`media`]: one module per widget, each with its data
//!   and its drawing code. Data providers are shared: one sampler feeds every
//!   system widget, one media listener feeds every player.
//! - [`layout`]: where each widget sits on the desktop.
//! - [`host`]: creates the windows, redraws a widget only when its data
//!   changes, and recolours everything when the theme changes.
//!
//! The widget file format and the Luau runtime from the plan arrive with the
//! creator tools; these three widgets are drawn natively.

pub mod bar;
pub mod clock;
mod edit;
pub mod flyout;
pub mod governor;
pub mod launcher;
pub mod host;
pub mod layout;
pub mod media;
pub mod home_assistant;
pub mod paint;
pub mod script;
pub mod parts;
pub mod snap;
pub mod stats;
pub mod weather;

/// The fonts shipped with Lumen, for [`lumen_render::Gpu::new`].
pub const BUNDLED_FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/inter-latin-400-normal.woff2"),
    include_bytes!("../../../assets/fonts/inter-latin-500-normal.woff2"),
    include_bytes!("../../../assets/fonts/inter-latin-600-normal.woff2"),
    include_bytes!("../../../assets/fonts/space-grotesk-latin-500-normal.woff2"),
    include_bytes!("../../../assets/fonts/space-grotesk-latin-600-normal.woff2"),
    // The handwritten face the app offers as a theme font; without it the desktop fell back to Segoe UI.
    include_bytes!("../../../assets/fonts/caveat-latin-600-normal.woff2"),
    include_bytes!("../../../assets/fonts/caveat-latin-700-normal.woff2"),
];

#[cfg(test)]
mod font_tests {
    /// Every font the settings app can preview a widget in must also be one the desktop can draw.
    #[test]
    fn the_desktop_has_every_font_the_app_offers() {
        let bundled = lumen_render::font_families_in(super::BUNDLED_FONTS);
        for family in ["Inter", "Space Grotesk", "Caveat"] {
            assert!(bundled.iter().any(|have| have == family), "{family} is missing from {bundled:?}");
        }
    }
}
