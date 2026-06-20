//! Input conditioning for dictation capture: a channel-aware [`Downmixer`] and an
//! adaptive [`AutoGain`] (AGC).
//!
//! Both run inside the realtime cpal callback on plain interleaved/mono `f32`, so
//! they stay decoupled from cpal's sample types and are unit-testable on their
//! own. The capture pipeline is: interleaved `f32` → [`Downmixer::mix`] → mono →
//! [`AutoGain::process`] (microphone only) → segmenter.

use crate::{peak_level, rms_level};

/// How quickly a channel's energy estimate tracks the signal (EMA weight).
const ENERGY_ADAPT: f32 = 0.1;
/// A channel counts as carrying signal when its energy is at least this fraction
/// of the loudest channel's — below it the channel is treated as dead/unpatched.
const CHANNEL_REL_FLOOR: f32 = 0.125;
/// Absolute energy floor so that during silence no channel is "active" (we then
/// fall back to a plain average, which is silence anyway).
const CHANNEL_ABS_FLOOR: f32 = 1e-4;

/// Downmixes interleaved multi-channel PCM to mono, *channel-aware*: it tracks
/// each channel's recent energy and averages only the channels that actually
/// carry signal.
///
/// This matters because a mono source is often wired to a single leg of a stereo
/// input (USB mics, audio interfaces, "Line In L"): a naive `sum / channels`
/// then halves it (-6 dB) by averaging in a dead channel. Here a left-only or
/// right-only source passes through at full level, while a genuine stereo source
/// still averages both legs.
pub struct Downmixer {
    channels: usize,
    /// Per-channel RMS energy, smoothed across buffers (EMA).
    energy: Vec<f32>,
}

impl Downmixer {
    /// A downmixer for `channels`-channel interleaved input (clamped to ≥ 1).
    pub fn new(channels: usize) -> Self {
        let channels = channels.max(1);
        Self {
            channels,
            energy: vec![0.0; channels],
        }
    }

    /// Downmix one interleaved buffer to mono. `interleaved.len()` is expected to
    /// be a multiple of the channel count; a ragged tail frame is ignored.
    pub fn mix(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let ch = self.channels;
        if ch == 1 {
            return interleaved.to_vec();
        }
        let frames = interleaved.len() / ch;
        if frames == 0 {
            return Vec::new();
        }

        // Per-channel RMS for this buffer, folded into the smoothed estimate.
        let mut sq = vec![0.0f32; ch];
        for frame in interleaved.chunks_exact(ch) {
            for (c, &s) in frame.iter().enumerate() {
                sq[c] += s * s;
            }
        }
        for c in 0..ch {
            let rms = (sq[c] / frames as f32).sqrt();
            self.energy[c] = self.energy[c] * (1.0 - ENERGY_ADAPT) + rms * ENERGY_ADAPT;
        }

        // Active = above the absolute floor and a fraction of the loudest channel.
        let max_energy = self.energy.iter().copied().fold(0.0f32, f32::max);
        let threshold = (max_energy * CHANNEL_REL_FLOOR).max(CHANNEL_ABS_FLOOR);
        let active: Vec<usize> = (0..ch).filter(|&c| self.energy[c] >= threshold).collect();

        let mut mono = Vec::with_capacity(frames);
        if active.is_empty() {
            // Silence / not yet settled: plain average (it's silence anyway).
            for frame in interleaved.chunks_exact(ch) {
                mono.push(frame.iter().sum::<f32>() / ch as f32);
            }
        } else {
            let n = active.len() as f32;
            for frame in interleaved.chunks_exact(ch) {
                let sum: f32 = active.iter().map(|&c| frame[c]).sum();
                mono.push(sum / n);
            }
        }
        mono
    }

    /// The channels currently judged active (carrying signal). Mainly for tests
    /// and diagnostics; `0` is the left/first channel.
    pub fn active_channels(&self) -> Vec<usize> {
        let max_energy = self.energy.iter().copied().fold(0.0f32, f32::max);
        let threshold = (max_energy * CHANNEL_REL_FLOOR).max(CHANNEL_ABS_FLOOR);
        (0..self.channels)
            .filter(|&c| self.energy[c] >= threshold)
            .collect()
    }
}

/// Target loudness the AGC drives speech toward (RMS, ≈ -20 dBFS).
const TARGET_RMS: f32 = 0.1;
/// Most the AGC will boost a quiet signal (keeps room noise from reaching speech
/// level) and most it will attenuate a hot one.
const MAX_GAIN: f32 = 8.0;
const MIN_GAIN: f32 = 0.25;
/// Below this input RMS the buffer is treated as silence: gain is held, never
/// raised, so faint noise isn't pumped up to speech level.
const NOISE_GATE_RMS: f32 = 0.006;
/// Smoothing toward the target gain: fast when reducing (catch a loud passage
/// before it clips), slow when boosting (no audible pumping).
const ATTACK: f32 = 0.5;
const RELEASE: f32 = 0.04;
/// Never let the smoothed gain push a buffer's peak above this (clip headroom).
const CLIP_CEILING: f32 = 0.97;

/// Adaptive gain control for microphone input: scales the mono signal toward a
/// target loudness so a quiet mic is boosted and a hot one tamed, without
/// clipping. Not applied to loopback (system-audio) capture, which is already at
/// line level.
pub struct AutoGain {
    gain: f32,
}

impl Default for AutoGain {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl AutoGain {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current gain factor (for diagnostics/tests).
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Apply adaptive gain to `samples` in place.
    pub fn process(&mut self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }
        let rms = rms_level(samples);
        let peak = peak_level(samples);

        // Gain that would hit the target this buffer; hold during near-silence so
        // noise isn't amplified.
        let desired = if rms > NOISE_GATE_RMS {
            (TARGET_RMS / rms).clamp(MIN_GAIN, MAX_GAIN)
        } else {
            self.gain
        };
        let coeff = if desired < self.gain { ATTACK } else { RELEASE };
        self.gain += (desired - self.gain) * coeff;

        // Hard guard: keep this buffer's loudest sample under the clip ceiling
        // even if the smoothed gain hasn't caught up yet.
        if peak > 0.0 {
            let ceiling = CLIP_CEILING / peak;
            if self.gain > ceiling {
                self.gain = ceiling.max(MIN_GAIN);
            }
        }

        for s in samples.iter_mut() {
            *s = (*s * self.gain).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an interleaved stereo buffer from per-channel amplitudes.
    fn stereo(left: f32, right: f32, frames: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            v.push(left * sign);
            v.push(right * sign);
        }
        v
    }

    #[test]
    fn mono_input_passes_through() {
        let mut dm = Downmixer::new(1);
        assert_eq!(dm.mix(&[0.1, -0.2, 0.3]), vec![0.1, -0.2, 0.3]);
    }

    #[test]
    fn left_only_stereo_is_not_halved() {
        // A mono source on the left leg only: classic -6 dB trap for sum/channels.
        let mut dm = Downmixer::new(2);
        let mut last = Vec::new();
        for _ in 0..20 {
            last = dm.mix(&stereo(0.3, 0.0, 64));
        }
        // Only the left channel is active, so the mono level equals it (~0.3),
        // not the halved 0.15 a naive average would give.
        assert_eq!(dm.active_channels(), vec![0]);
        assert!((peak_level(&last) - 0.3).abs() < 1e-3, "peak was {}", peak_level(&last));
    }

    #[test]
    fn true_stereo_averages_both_channels() {
        let mut dm = Downmixer::new(2);
        let mut last = Vec::new();
        for _ in 0..20 {
            last = dm.mix(&stereo(0.2, 0.2, 64));
        }
        assert_eq!(dm.active_channels(), vec![0, 1]);
        assert!((peak_level(&last) - 0.2).abs() < 1e-3);
    }

    #[test]
    fn silence_falls_back_to_plain_average() {
        let mut dm = Downmixer::new(2);
        let out = dm.mix(&stereo(0.0, 0.0, 32));
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn auto_gain_boosts_a_quiet_signal() {
        let mut agc = AutoGain::new();
        let mut buf;
        for _ in 0..200 {
            buf = vec![0.02_f32; 256];
            agc.process(&mut buf);
        }
        // A 0.02-RMS input should be lifted toward the 0.1 target.
        assert!(agc.gain() > 2.0, "gain only reached {}", agc.gain());
        let mut probe = vec![0.02_f32; 256];
        agc.process(&mut probe);
        assert!(rms_level(&probe) > 0.05);
    }

    #[test]
    fn auto_gain_never_clips_a_hot_signal() {
        let mut agc = AutoGain::new();
        for n in 0..200 {
            let amp = if n % 2 == 0 { 0.8 } else { -0.8 };
            let mut buf = vec![amp; 256];
            agc.process(&mut buf);
            assert!(buf.iter().all(|&s| s.abs() <= 1.0), "clipped at buffer {n}");
        }
        assert!(agc.gain() < 1.0, "hot signal should be attenuated, gain {}", agc.gain());
    }

    #[test]
    fn auto_gain_holds_on_near_silence() {
        let mut agc = AutoGain::new();
        for _ in 0..200 {
            let mut buf = vec![0.001_f32; 256];
            agc.process(&mut buf);
        }
        // Sub-gate noise must not be pumped up toward the target.
        assert!((agc.gain() - 1.0).abs() < 1e-3, "noise lifted gain to {}", agc.gain());
    }
}
