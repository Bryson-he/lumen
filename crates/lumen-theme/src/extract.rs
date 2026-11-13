use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageReader};
use material_colors::{
    color::Rgb,
    quantize::{Quantizer, QuantizerCelebi},
    score::Score,
};

use crate::{Color, Error};

/// The image is sampled on a grid of about this many points per side. Palette
/// extraction only needs colour proportions, so ~16k samples is plenty and keeps
/// the quantiser in the low milliseconds whatever the wallpaper's resolution.
const SAMPLE_GRID: u32 = 128;

/// Material's stand-in seed when an image has no usable colour.
const FALLBACK: Rgb = Rgb::new(66, 133, 244);

/// The colours found in a wallpaper, best theme seed first. Cheap to store next
/// to the wallpaper so switching themes never re-reads the image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceColors {
    /// Ranked seeds: `candidates[0]` is the one themes are built from.
    /// Never empty.
    pub candidates: Vec<Color>,
}

impl SourceColors {
    /// A single picked colour, with no wallpaper behind it.
    pub fn from_color(color: Color) -> Self {
        Self { candidates: vec![color] }
    }

    /// The colour themes are generated from.
    pub fn seed(&self) -> Color {
        self.candidates[0]
    }
}

/// Reads an image file and finds its source colours.
pub fn extract(path: impl AsRef<Path>) -> Result<SourceColors, Error> {
    let path = path.as_ref();
    let image_err = |source| Error::Image { path: path.display().to_string(), source };
    let image = ImageReader::open(path)
        .map_err(|e| image_err(e.into()))?
        .with_guessed_format()
        .map_err(|e| image_err(e.into()))?
        .decode()
        .map_err(image_err)?;
    extract_from_image(&image)
}

/// Finds the source colours of an already-decoded image.
pub fn extract_from_image(image: &DynamicImage) -> Result<SourceColors, Error> {
    let pixels = sample(image);
    if pixels.is_empty() {
        return Err(Error::EmptyImage);
    }

    let quantized = QuantizerCelebi::quantize(&pixels, 128).color_to_count;

    // Prefer colourful, well-represented colours. If the image has none (a grey
    // or near-monochrome wallpaper), fall back to whatever it does contain so
    // the theme still takes its tint from the image rather than a stock blue.
    let mut ranked = Score::score(&quantized, Some(4), Some(FALLBACK), Some(true));
    if ranked == [FALLBACK] && !quantized.contains_key(&FALLBACK) {
        ranked = Score::score(&quantized, Some(4), Some(FALLBACK), Some(false));
    }

    Ok(SourceColors { candidates: ranked.into_iter().map(Color::from).collect() })
}

/// Samples the image on a regular grid, skipping mostly-transparent pixels.
fn sample(image: &DynamicImage) -> Vec<Rgb> {
    let (width, height) = image.dimensions();
    let step = width.max(height).div_ceil(SAMPLE_GRID).max(1);
    let mut pixels = Vec::with_capacity(((width / step + 1) * (height / step + 1)) as usize);
    for y in (step / 2..height).step_by(step as usize) {
        for x in (step / 2..width).step_by(step as usize) {
            let [r, g, b, a] = image.get_pixel(x, y).0;
            if a >= 128 {
                pixels.push(Rgb::new(r, g, b));
            }
        }
    }
    pixels
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{Rgb as Px, RgbImage, Rgba, RgbaImage};
    use material_colors::hct::Hct;

    use super::*;

    pub(crate) fn hue(c: Color) -> f64 {
        Hct::new(c.into()).get_hue()
    }

    /// Smallest angle between two hues, in degrees.
    pub(crate) fn hue_distance(a: f64, b: f64) -> f64 {
        let d = (a - b).abs() % 360.0;
        d.min(360.0 - d)
    }

    /// The "Midnight Harbour" wallpaper from the designs: navy sky, blue hills,
    /// dark water and a warm yellow sun.
    pub(crate) fn harbour(width: u32, height: u32) -> DynamicImage {
        let (sky, hill, water, sun) = (Px([0x1B, 0x24, 0x36]), Px([0x3B, 0x5B, 0x8C]), Px([0x26, 0x34, 0x4F]), Px([0xF2, 0xCC, 0x8F]));
        let (w, h) = (f64::from(width), f64::from(height));
        DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            let (fx, fy) = (f64::from(x) / w, f64::from(y) / h);
            let in_sun = ((fx - 0.75) * w / h).hypot(fy - 0.26) < 0.14;
            if fy > 0.73 {
                water
            } else if fy > 0.45 + 0.12 * (fx * 19.0).sin() {
                hill
            } else if in_sun {
                sun
            } else {
                sky
            }
        }))
    }

    fn solid(color: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, Px(color)))
    }

    #[test]
    fn solid_colour_image_seeds_that_hue() {
        let red = extract_from_image(&solid([200, 40, 40])).unwrap();
        assert!(hue_distance(hue(red.seed()), hue(Color::new(200, 40, 40))) < 5.0);
    }

    #[test]
    fn harbour_seed_is_blue_and_the_sun_is_a_candidate() {
        let colors = extract_from_image(&harbour(640, 360)).unwrap();
        assert!(Hct::is_blue(hue(colors.seed())), "seed {} should be blue", colors.seed());
        let sun_hue = hue(Color::new(0xF2, 0xCC, 0x8F));
        assert!(
            colors.candidates.iter().any(|c| hue_distance(hue(*c), sun_hue) < 15.0),
            "expected a warm candidate in {:?}",
            colors.candidates
        );
    }

    #[test]
    fn grey_image_does_not_fall_back_to_stock_blue() {
        let colors = extract_from_image(&solid([128, 126, 120])).unwrap();
        assert_ne!(colors.seed(), Color::from(FALLBACK));
    }

    #[test]
    fn extraction_is_deterministic_and_resolution_independent() {
        let small = extract_from_image(&harbour(640, 360)).unwrap();
        assert_eq!(small, extract_from_image(&harbour(640, 360)).unwrap());
        let large = extract_from_image(&harbour(3840, 2160)).unwrap();
        assert!(hue_distance(hue(small.seed()), hue(large.seed())) < 10.0);
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        let image = RgbaImage::from_fn(64, 64, |x, _| if x < 32 { Rgba([255, 0, 0, 0]) } else { Rgba([40, 160, 60, 255]) });
        let colors = extract_from_image(&DynamicImage::ImageRgba8(image)).unwrap();
        assert!(hue_distance(hue(colors.seed()), hue(Color::new(40, 160, 60))) < 5.0);
    }

    #[test]
    fn empty_or_fully_transparent_images_are_errors() {
        let clear = DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0])));
        assert!(matches!(extract_from_image(&clear), Err(Error::EmptyImage)));
        assert!(matches!(extract_from_image(&DynamicImage::new_rgb8(0, 0)), Err(Error::EmptyImage)));
    }

    #[test]
    fn missing_file_reports_the_path() {
        let err = extract("no-such-wallpaper.jpg").unwrap_err().to_string();
        assert!(err.contains("no-such-wallpaper.jpg"), "{err}");
    }
}
