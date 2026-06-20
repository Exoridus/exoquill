//! Screen-region capture for desktop OCR (decisions D4, pulled forward to v0.1).
//!
//! Grabs the monitor under a screen point with `xcap` and returns it as an
//! `image` [`RgbaImage`] (physical pixels) plus the monitor's logical geometry +
//! DPI scale. The caller places a fullscreen selection overlay over that monitor
//! (logical coords) and maps the logical selection rectangle back to physical
//! pixels (via [`ScreenShot::scale`]) to crop the captured image.

use image::{ImageEncoder, RgbaImage};
use xcap::Monitor;

/// A captured monitor: physical-pixel image plus the geometry needed to overlay
/// a selection window and map the selection back to pixels.
pub struct ScreenShot {
    /// Captured pixels at the monitor's physical resolution.
    pub image: RgbaImage,
    /// Physical-per-logical pixel ratio (DPI scale) of the monitor.
    pub scale: f32,
    /// Monitor top-left in logical coordinates (overlay window position).
    pub logical_x: f64,
    pub logical_y: f64,
    /// Monitor size in logical coordinates (overlay window size).
    pub logical_width: f64,
    pub logical_height: f64,
}

/// Capture the monitor containing the physical point `(x, y)` (e.g. the cursor).
pub fn capture_at_point(x: i32, y: i32) -> Result<ScreenShot, String> {
    let monitor = Monitor::from_point(x, y).map_err(|e| format!("find monitor: {e}"))?;
    let scale = monitor
        .scale_factor()
        .map_err(|e| format!("scale factor: {e}"))?;
    let mx = monitor.x().map_err(|e| format!("monitor x: {e}"))?;
    let my = monitor.y().map_err(|e| format!("monitor y: {e}"))?;

    let captured = monitor
        .capture_image()
        .map_err(|e| format!("capture screen: {e}"))?;
    let (w, h) = (captured.width(), captured.height());
    // Convert via raw RGBA bytes so this crate's `image` version is independent
    // of xcap's (both are RGBA8 row-major, so the bytes transfer directly).
    let image = RgbaImage::from_raw(w, h, captured.into_raw())
        .ok_or_else(|| "captured image buffer size mismatch".to_string())?;

    let scale64 = scale as f64;
    Ok(ScreenShot {
        image,
        scale,
        logical_x: mx as f64 / scale64,
        logical_y: my as f64 / scale64,
        logical_width: w as f64 / scale64,
        logical_height: h as f64 / scale64,
    })
}

impl ScreenShot {
    /// The full screenshot as PNG bytes (for the overlay to display).
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        encode_png(&self.image)
    }

    /// Crop a selection rectangle given in the overlay's logical coordinates
    /// (relative to the monitor's top-left) and return it as PNG bytes. Maps
    /// logical → physical pixels via the DPI [`scale`](Self::scale) and clamps to
    /// the image bounds. `Ok(None)` if the selection is empty or off-screen.
    pub fn crop_png(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<Option<Vec<u8>>, String> {
        let s = self.scale as f64;
        let px = (x * s).round().max(0.0) as u32;
        let py = (y * s).round().max(0.0) as u32;
        let pw = (width * s).round().max(0.0) as u32;
        let ph = (height * s).round().max(0.0) as u32;
        let (iw, ih) = (self.image.width(), self.image.height());
        if pw == 0 || ph == 0 || px >= iw || py >= ih {
            return Ok(None);
        }
        let cw = pw.min(iw - px);
        let ch = ph.min(ih - py);
        let cropped = image::imageops::crop_imm(&self.image, px, py, cw, ch).to_image();
        Ok(Some(encode_png(&cropped)?))
    }
}

/// Encode an RGBA image as PNG bytes (the `png` codec is enabled in Cargo.toml).
fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("encode png: {e}"))?;
    Ok(out)
}
