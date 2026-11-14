//! Wallpaper -> palette -> roles -> tokens.
//!
//! Two steps, kept separate so the slow one can be cached per wallpaper:
//!
//! 1. [`extract`] reads an image and returns its [`SourceColors`] (tens of milliseconds).
//! 2. [`Theme::generate`] turns a seed colour into the eight theme roles (microseconds).
//!
//! The JSON shape of [`Theme`] is the theme file format in `docs/PLAN.md` -> "File formats".

mod color;
pub mod export;
mod extract;
mod generate;
mod theme;

pub use color::Color;
pub use extract::{SourceColors, extract, extract_from_image};
pub use theme::{Blur, Mode, Motion, Roles, Source, Style, Theme, Variant};

/// Everything that can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read image {path}: {source}")]
    Image { path: String, source: image::ImageError },
    #[error("the image has no pixels")]
    EmptyImage,
    #[error("\"{0}\" is not a colour; expected #RRGGBB")]
    BadColor(String),
    #[error("unknown {what} \"{value}\"; expected one of: {expected}")]
    BadOption { what: &'static str, value: String, expected: &'static str },
    #[error("contrast must be -1 (reduced), 0 (standard) or 1 (high), got {0}")]
    BadContrast(i64),
}
