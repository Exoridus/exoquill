//! Energy-based speech segmentation for live dictation.
//!
//! The dictation worker feeds captured mono PCM buffers (at the capture sample
//! rate) into a [`Segmenter`], which emits a finalized utterance whenever a run
//! of speech is followed by a short pause — or when an utterance grows past a
//! safety cap. Committing on natural phrase pauses is what makes dictation feel
//! live: short pieces of text land in the note as you speak.
//!
//! An adaptive noise floor keeps it working across microphones and rooms
//! without a hand-tuned per-user threshold. This is deliberately a cheap energy
//! VAD; a neural VAD (Silero) is a v0.2 improvement (see `docs/roadmap.md`).

use std::collections::VecDeque;

use crate::rms_level;

/// Audio kept before speech onset so a segment isn't clipped at the start.
const PREROLL_MS: u32 = 250;
/// Trailing silence (while speaking) that finalizes an utterance.
const HANGOVER_MS: u32 = 500;
/// Utterances shorter than this are dropped (clicks, coughs, lip smacks).
const MIN_UTTERANCE_MS: u32 = 200;
/// Force-finalize nonstop speech so text keeps flowing and memory stays bounded.
const MAX_UTTERANCE_MS: u32 = 12_000;
/// A buffer counts as speech when its RMS exceeds `noise_floor * NOISE_MARGIN`.
const NOISE_MARGIN: f32 = 2.0;
/// Absolute lower bound on the speech threshold so silence never trips it.
const MIN_THRESHOLD: f32 = 0.012;
/// How quickly the noise floor tracks the ambient level (EMA weight).
const NOISE_ADAPT: f32 = 0.05;

/// Stateful energy VAD that turns a stream of PCM buffers into utterances.
pub struct Segmenter {
    rate: u32,
    preroll_cap: usize,
    hangover_samples: usize,
    min_samples: usize,
    max_samples: usize,
    speaking: bool,
    silence_run: usize,
    /// Samples added while actually speaking (excludes pre-roll and the trailing
    /// hangover silence), used for the minimum-length gate.
    speech_samples: usize,
    noise_floor: f32,
    last_level: f32,
    utterance: Vec<f32>,
    preroll: VecDeque<f32>,
}

impl Segmenter {
    /// Create a segmenter for audio captured at `rate` Hz (mono).
    pub fn new(rate: u32) -> Self {
        let ms = |m: u32| (m as u64 * rate.max(1) as u64 / 1000) as usize;
        Self {
            rate,
            preroll_cap: ms(PREROLL_MS),
            hangover_samples: ms(HANGOVER_MS),
            min_samples: ms(MIN_UTTERANCE_MS),
            max_samples: ms(MAX_UTTERANCE_MS),
            speaking: false,
            silence_run: 0,
            speech_samples: 0,
            noise_floor: MIN_THRESHOLD,
            last_level: 0.0,
            utterance: Vec::new(),
            preroll: VecDeque::new(),
        }
    }

    /// The sample rate this segmenter was built for.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The most recent input level (RMS), for driving an input meter.
    pub fn level(&self) -> f32 {
        self.last_level
    }

    /// Feed one captured buffer. Returns a finalized utterance (mono samples at
    /// the capture rate) when a speech run just ended, otherwise `None`.
    pub fn push(&mut self, frame: &[f32]) -> Option<Vec<f32>> {
        if frame.is_empty() {
            return None;
        }
        let level = rms_level(frame);
        self.last_level = level;
        let threshold = (self.noise_floor * NOISE_MARGIN).max(MIN_THRESHOLD);

        if level > threshold {
            if !self.speaking {
                self.speaking = true;
                self.utterance.clear();
                self.utterance.extend(self.preroll.drain(..));
            }
            self.utterance.extend_from_slice(frame);
            self.speech_samples += frame.len();
            self.silence_run = 0;
        } else {
            // Track the ambient level only while not in a speech run.
            self.noise_floor = self.noise_floor * (1.0 - NOISE_ADAPT) + level * NOISE_ADAPT;
            if self.speaking {
                self.utterance.extend_from_slice(frame);
                self.silence_run += frame.len();
            } else {
                self.preroll.extend(frame.iter().copied());
                while self.preroll.len() > self.preroll_cap {
                    self.preroll.pop_front();
                }
            }
        }

        let ended_by_pause = self.speaking && self.silence_run >= self.hangover_samples;
        let ended_by_cap = self.speaking && self.utterance.len() >= self.max_samples;
        if ended_by_pause || ended_by_cap {
            self.finalize()
        } else {
            None
        }
    }

    /// Finalize an in-progress utterance, e.g. when dictation is stopped.
    pub fn flush(&mut self) -> Option<Vec<f32>> {
        if self.speaking {
            self.finalize()
        } else {
            None
        }
    }

    fn finalize(&mut self) -> Option<Vec<f32>> {
        self.speaking = false;
        self.silence_run = 0;
        self.preroll.clear();
        let speech = std::mem::take(&mut self.speech_samples);
        let utterance = std::mem::take(&mut self.utterance);
        (speech >= self.min_samples).then_some(utterance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    /// A buffer of `ms` milliseconds at `amplitude` (steady tone-ish energy).
    fn buf(ms: u32, amplitude: f32) -> Vec<f32> {
        let n = (ms as u64 * RATE as u64 / 1000) as usize;
        (0..n)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect()
    }

    #[test]
    fn silence_never_emits() {
        let mut seg = Segmenter::new(RATE);
        for _ in 0..50 {
            assert!(seg.push(&buf(20, 0.0)).is_none());
        }
    }

    #[test]
    fn speech_then_pause_emits_one_utterance() {
        let mut seg = Segmenter::new(RATE);
        // ~600 ms of speech...
        let mut emitted = None;
        for _ in 0..20 {
            if let Some(u) = seg.push(&buf(30, 0.3)) {
                emitted = Some(u);
            }
        }
        assert!(
            emitted.is_none(),
            "should not finalize while still speaking"
        );
        // ...then ~600 ms of silence finalizes it.
        for _ in 0..20 {
            if let Some(u) = seg.push(&buf(30, 0.0)) {
                emitted = Some(u);
            }
        }
        let utterance = emitted.expect("an utterance should have been finalized");
        assert!(!utterance.is_empty());
    }

    #[test]
    fn short_blip_is_dropped() {
        let mut seg = Segmenter::new(RATE);
        // 50 ms blip (< MIN_UTTERANCE_MS) then silence.
        let mut emitted = None;
        if let Some(u) = seg.push(&buf(50, 0.4)) {
            emitted = Some(u);
        }
        for _ in 0..30 {
            if let Some(u) = seg.push(&buf(30, 0.0)) {
                emitted = Some(u);
            }
        }
        assert!(emitted.is_none(), "a sub-threshold-length blip is dropped");
    }

    #[test]
    fn nonstop_speech_force_finalizes() {
        let mut seg = Segmenter::new(RATE);
        let mut emitted = None;
        // 13 s of continuous speech exceeds MAX_UTTERANCE_MS (12 s).
        for _ in 0..130 {
            if let Some(u) = seg.push(&buf(100, 0.3)) {
                emitted = Some(u);
            }
        }
        assert!(
            emitted.is_some(),
            "long speech must force-finalize a segment"
        );
    }

    #[test]
    fn flush_returns_trailing_utterance() {
        let mut seg = Segmenter::new(RATE);
        for _ in 0..15 {
            seg.push(&buf(30, 0.3));
        }
        let trailing = seg.flush().expect("trailing speech should flush");
        assert!(!trailing.is_empty());
        assert!(seg.flush().is_none(), "nothing left after a flush");
    }
}
