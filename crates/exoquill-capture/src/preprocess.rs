//! Image preprocessing for OCR.
//!
//! Captured images often have low resolution and poor contrast, both of which
//! hurt OCR accuracy. This module exposes a small pipeline that decodes an
//! encoded image (PNG/JPEG), converts it to grayscale, upscales it and boosts
//! contrast before re-encoding it as a PNG.
//!
//! The high-level entry point is [`preprocess_for_ocr`]. The individual steps
//! ([`to_grayscale`], [`upscale_2x`], [`adjust_contrast`]) are also public so
//! they can be composed or tested in isolation.

use std::error::Error;
use std::fmt;
use std::io::Cursor;

use image::imageops::FilterType;
use image::{GrayImage, ImageError, ImageFormat};

/// Factor by which images are upscaled. OCR engines generally perform better on
/// larger glyphs, and 2x is a good trade-off between accuracy and cost.
const UPSCALE_FACTOR: u32 = 2;

/// Default contrast boost applied by [`preprocess_for_ocr`]. Positive values
/// increase contrast; see [`adjust_contrast`].
const DEFAULT_CONTRAST: f32 = 30.0;

/// Errors that can occur while preprocessing an image for OCR.
#[derive(Debug)]
pub enum PreprocessError {
    /// The input bytes could not be decoded as a supported image format.
    Decode(ImageError),
    /// The processed image could not be re-encoded as a PNG.
    Encode(ImageError),
    /// The decoded image had a zero width or height and cannot be processed.
    EmptyImage,
}

impl fmt::Display for PreprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreprocessError::Decode(err) => write!(f, "failed to decode image: {err}"),
            PreprocessError::Encode(err) => write!(f, "failed to encode image: {err}"),
            PreprocessError::EmptyImage => write!(f, "image has zero width or height"),
        }
    }
}

impl Error for PreprocessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PreprocessError::Decode(err) | PreprocessError::Encode(err) => Some(err),
            PreprocessError::EmptyImage => None,
        }
    }
}

/// Decode an encoded image, clean it up for OCR and re-encode it as a PNG.
///
/// The pipeline is: decode -> grayscale -> upscale 2x -> contrast boost ->
/// encode as PNG. Both PNG and JPEG inputs are supported.
///
/// # Errors
///
/// Returns [`PreprocessError::Decode`] if `bytes` is not a supported image,
/// [`PreprocessError::EmptyImage`] if the decoded image has no pixels, and
/// [`PreprocessError::Encode`] if PNG encoding fails.
///
/// # Examples
///
/// ```no_run
/// # fn run(raw: &[u8]) -> Result<(), exoquill_capture::PreprocessError> {
/// let png = exoquill_capture::preprocess_for_ocr(raw)?;
/// assert!(!png.is_empty());
/// # Ok(())
/// # }
/// ```
pub fn preprocess_for_ocr(bytes: &[u8]) -> Result<Vec<u8>, PreprocessError> {
    let decoded = image::load_from_memory(bytes).map_err(PreprocessError::Decode)?;

    let gray = to_grayscale(&decoded);
    if gray.width() == 0 || gray.height() == 0 {
        return Err(PreprocessError::EmptyImage);
    }

    let upscaled = upscale_2x(&gray);
    let contrasted = adjust_contrast(&upscaled, DEFAULT_CONTRAST);

    encode_png(&contrasted)
}

/// Convert any decoded image to an 8-bit grayscale buffer.
pub fn to_grayscale(image: &image::DynamicImage) -> GrayImage {
    image.to_luma8()
}

/// Upscale a grayscale image by a fixed factor of 2 using Lanczos resampling,
/// which keeps edges crisp for OCR.
///
/// Dimensions are saturated to avoid overflow on pathologically large inputs.
pub fn upscale_2x(image: &GrayImage) -> GrayImage {
    let width = image.width().saturating_mul(UPSCALE_FACTOR);
    let height = image.height().saturating_mul(UPSCALE_FACTOR);
    image::imageops::resize(image, width, height, FilterType::Lanczos3)
}

/// Adjust the contrast of a grayscale image.
///
/// `contrast` is passed straight through to [`image::imageops::contrast`]:
/// positive values increase contrast and negative values decrease it.
pub fn adjust_contrast(image: &GrayImage, contrast: f32) -> GrayImage {
    image::imageops::contrast(image, contrast)
}

/// Encode a grayscale image as PNG bytes.
fn encode_png(image: &GrayImage) -> Result<Vec<u8>, PreprocessError> {
    let mut buffer = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
        .map_err(PreprocessError::Encode)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    /// Build a tiny 2x2 RGB image with distinct colours so grayscale and
    /// contrast changes are observable.
    fn sample_rgb() -> RgbImage {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, Rgb([0, 0, 0])); // black
        img.put_pixel(1, 0, Rgb([255, 255, 255])); // white
        img.put_pixel(0, 1, Rgb([128, 128, 128])); // mid gray
        img.put_pixel(1, 1, Rgb([200, 50, 50])); // reddish
        img
    }

    /// Encode an `RgbImage` to PNG bytes for use as pipeline input.
    fn encode_rgb_png(img: &RgbImage) -> Vec<u8> {
        let mut buffer = Vec::new();
        img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
            .expect("encoding test image must succeed");
        buffer
    }

    #[test]
    fn grayscale_preserves_dimensions_and_known_values() {
        let dynamic = DynamicImage::ImageRgb8(sample_rgb());
        let gray = to_grayscale(&dynamic);

        assert_eq!(gray.dimensions(), (2, 2));
        assert_eq!(gray.get_pixel(0, 0)[0], 0); // black -> 0
        assert_eq!(gray.get_pixel(1, 0)[0], 255); // white -> 255
    }

    #[test]
    fn upscale_doubles_dimensions() {
        let gray = DynamicImage::ImageRgb8(sample_rgb()).to_luma8();
        let upscaled = upscale_2x(&gray);

        assert_eq!(upscaled.dimensions(), (4, 4));
    }

    #[test]
    fn contrast_pushes_values_to_extremes() {
        let gray = DynamicImage::ImageRgb8(sample_rgb()).to_luma8();
        let boosted = adjust_contrast(&gray, 100.0);

        // The mid-gray pixel should move away from 128 under a strong boost.
        let original_mid = gray.get_pixel(0, 1)[0];
        let boosted_mid = boosted.get_pixel(0, 1)[0];
        assert_ne!(original_mid, boosted_mid);

        // Pure black/white stay clamped at the extremes.
        assert_eq!(boosted.get_pixel(0, 0)[0], 0);
        assert_eq!(boosted.get_pixel(1, 0)[0], 255);
    }

    #[test]
    fn pipeline_outputs_2x_grayscale_png() {
        let input = encode_rgb_png(&sample_rgb());
        let output = preprocess_for_ocr(&input).expect("preprocessing must succeed");

        // The output must decode back into a valid PNG.
        let decoded = image::load_from_memory_with_format(&output, ImageFormat::Png)
            .expect("output must be a valid PNG");

        // It must be 2x larger in both dimensions...
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 4);

        // ...and grayscale: every pixel's channels must be equal.
        let rgb = decoded.to_rgb8();
        for pixel in rgb.pixels() {
            let [r, g, b] = pixel.0;
            assert_eq!(r, g);
            assert_eq!(g, b);
        }
    }

    #[test]
    fn pipeline_accepts_jpeg_input() {
        let mut buffer = Vec::new();
        sample_rgb()
            .write_to(&mut Cursor::new(&mut buffer), ImageFormat::Jpeg)
            .expect("encoding JPEG test image must succeed");

        let output = preprocess_for_ocr(&buffer).expect("JPEG input must be supported");
        let decoded = image::load_from_memory_with_format(&output, ImageFormat::Png)
            .expect("output must be a valid PNG");
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }

    #[test]
    fn invalid_bytes_yield_decode_error() {
        let err = preprocess_for_ocr(&[0u8, 1, 2, 3]).unwrap_err();
        assert!(matches!(err, PreprocessError::Decode(_)));
        // Exercise Display/Error impls.
        assert!(!err.to_string().is_empty());
        assert!(std::error::Error::source(&err).is_some());
    }
}
