//! Audio plumbing for ExoQuill.
//!
//! Microphone input, gain/normalization and the ring buffer feeding VAD
//! segmentation (see `docs/roadmap.md`, PR 5).
//!
//! This module currently provides device enumeration over the platform's
//! default audio host plus small, dependency-free helpers for computing
//! input-level metrics (RMS and peak) that drive a live input meter.
//! Actual capture streams and the ring buffer are intentionally out of
//! scope here and will be added alongside whisper integration.

use cpal::traits::{DeviceTrait, HostTrait};

/// Returns the names of the available audio *input* devices.
///
/// Enumeration goes through cpal's [default host](cpal::default_host). The
/// call is deliberately robust: on a headless machine (e.g. CI) with no
/// host or no devices it returns an empty [`Vec`] instead of panicking.
/// Devices whose name cannot be queried are silently skipped.
///
/// # Examples
///
/// ```
/// // On a machine without audio hardware this is simply empty.
/// let devices = exoquill_audio::list_input_devices();
/// assert!(devices.iter().all(|name| !name.is_empty()));
/// ```
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device_name(&device)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Returns the name of the system's default audio *input* device, if any.
///
/// Resolves the default input device via cpal's
/// [default host](cpal::default_host). Returns [`None`] when there is no
/// default input device (e.g. headless CI) or when its name cannot be
/// queried. This function never panics.
///
/// # Examples
///
/// ```
/// // Just make sure calling it is side-effect free and never panics.
/// let _ = exoquill_audio::default_input_device_name();
/// ```
pub fn default_input_device_name() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .and_then(|device| device_name(&device))
}

/// Resolves a human-readable name for a cpal [`Device`](cpal::Device).
///
/// In cpal 0.18 a device's name lives in its structured
/// [`DeviceDescription`](cpal::DeviceDescription); the [`Display`] impl can
/// fail when the description cannot be queried (e.g. the device was just
/// unplugged), which would make `to_string()` panic. We therefore go through
/// the fallible `description()` and return [`None`] on failure so callers
/// simply skip such devices.
///
/// [`Display`]: std::fmt::Display
fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_owned())
}

/// Computes the root-mean-square (RMS) amplitude of mono PCM `samples`.
///
/// RMS is a good proxy for perceived loudness and is used to drive the
/// input-level meter. Samples are expected to be normalized floating-point
/// PCM in roughly `[-1.0, 1.0]`, and the result is in the same units.
///
/// An empty slice returns `0.0`.
///
/// # Examples
///
/// ```
/// use exoquill_audio::rms_level;
///
/// assert_eq!(rms_level(&[]), 0.0);
/// assert_eq!(rms_level(&[0.0; 8]), 0.0);
/// // A constant signal has an RMS equal to its absolute level.
/// assert!((rms_level(&[0.5; 8]) - 0.5).abs() < 1e-6);
/// ```
pub fn rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_of_squares: f32 = samples.iter().map(|&sample| sample * sample).sum();
    (sum_of_squares / samples.len() as f32).sqrt()
}

/// Computes the peak (maximum absolute) amplitude of mono PCM `samples`.
///
/// This complements [`rms_level`] and is useful for clip detection in an
/// input-level meter. Samples are expected to be normalized floating-point
/// PCM in roughly `[-1.0, 1.0]`.
///
/// An empty slice returns `0.0`.
///
/// # Examples
///
/// ```
/// use exoquill_audio::peak_level;
///
/// assert_eq!(peak_level(&[]), 0.0);
/// assert_eq!(peak_level(&[0.0; 8]), 0.0);
/// assert_eq!(peak_level(&[-0.8, 0.3, 0.5]), 0.8);
/// ```
pub fn peak_level(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerance for floating-point comparisons of level metrics.
    const EPSILON: f32 = 1e-6;

    #[test]
    fn rms_of_empty_is_zero() {
        assert_eq!(rms_level(&[]), 0.0);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms_level(&[0.0; 64]), 0.0);
    }

    #[test]
    fn rms_of_constant_signal_equals_level() {
        // The RMS of a constant signal is its absolute value, and for a
        // positive constant the peak matches it as well.
        let signal = [0.5_f32; 32];
        assert!((rms_level(&signal) - 0.5).abs() < EPSILON);
        assert_eq!(peak_level(&signal), 0.5);
    }

    #[test]
    fn rms_of_known_signal_matches_expected() {
        // A symmetric square-like signal alternating +/- 0.25 has an RMS
        // equal to 0.25 regardless of length.
        let signal = [0.25_f32, -0.25, 0.25, -0.25];
        assert!((rms_level(&signal) - 0.25).abs() < EPSILON);

        // A two-sample signal of {3, 4} (scaled down) has a known RMS of
        // sqrt((9 + 16) / 2) = sqrt(12.5) ~= 3.5355.
        let pythagorean = [0.3_f32, 0.4];
        let expected = (0.125_f32).sqrt();
        assert!((rms_level(&pythagorean) - expected).abs() < EPSILON);
    }

    #[test]
    fn peak_of_empty_is_zero() {
        assert_eq!(peak_level(&[]), 0.0);
    }

    #[test]
    fn peak_of_silence_is_zero() {
        assert_eq!(peak_level(&[0.0; 64]), 0.0);
    }

    #[test]
    fn peak_picks_largest_magnitude_regardless_of_sign() {
        assert_eq!(peak_level(&[-0.9, 0.4, 0.1, -0.2]), 0.9);
    }
}
