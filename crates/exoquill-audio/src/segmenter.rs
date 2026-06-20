//! Energy-based speech segmentation for live dictation.
//!
//! The dictation worker feeds captured mono PCM buffers (at the capture sample
//! rate) into a [`Segmenter`], which emits a finalized utterance whenever a run
//! of speech is followed by a short pause — or when an utterance grows past a
//! safety cap. Committing on natural phrase pauses is what makes dictation feel
//! live: short pieces of text land in the note as you speak.
//!
//! An adaptive noise floor keeps it working across microphones and rooms
//! without a hand-tuned per-user threshold. The speech/non-speech decision is
//! factored out behind [`SpeechGate`] so a neural VAD (Silero) can replace the
//! cheap energy gate without touching the buffering logic here.

use std::collections::VecDeque;

use crate::rms_level;

/// Decides, per captured frame, whether it contains speech — the pluggable front
/// end of the [`Segmenter`]. Implementations are stateful (a noise-floor EMA, or
/// a neural model carrying its recurrent state) so they take `&mut self`.
pub trait SpeechGate: Send {
    /// Whether `frame` (mono PCM at `rate` Hz) contains speech.
    fn is_speech(&mut self, frame: &[f32], rate: u32) -> bool;
    /// The most recent input level (roughly RMS, in `[0, 1]`) for the meter.
    fn level(&self) -> f32;
}

/// The default [`SpeechGate`]: a cheap energy VAD with an adaptive noise floor
/// (no per-user threshold). Robust enough for quiet rooms; a neural gate handles
/// noisy ones better.
pub struct EnergyGate {
    noise_floor: f32,
    last_level: f32,
}

impl Default for EnergyGate {
    fn default() -> Self {
        Self {
            noise_floor: MIN_THRESHOLD,
            last_level: 0.0,
        }
    }
}

impl EnergyGate {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SpeechGate for EnergyGate {
    fn is_speech(&mut self, frame: &[f32], _rate: u32) -> bool {
        if frame.is_empty() {
            return false;
        }
        let level = rms_level(frame);
        self.last_level = level;
        let threshold = (self.noise_floor * NOISE_MARGIN).max(MIN_THRESHOLD);
        if level > threshold {
            true
        } else {
            // Track the ambient level from sub-threshold frames.
            self.noise_floor = self.noise_floor * (1.0 - NOISE_ADAPT) + level * NOISE_ADAPT;
            false
        }
    }

    fn level(&self) -> f32 {
        self.last_level
    }
}

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

/// Turns a stream of PCM buffers into utterances. The speech decision is
/// delegated to a [`SpeechGate`] (energy by default, Silero when available);
/// this struct owns the pre-roll / hangover / length buffering around it.
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
    gate: Box<dyn SpeechGate>,
    utterance: Vec<f32>,
    preroll: VecDeque<f32>,
}

impl Segmenter {
    /// Create a segmenter for audio captured at `rate` Hz (mono) using the
    /// default energy gate.
    pub fn new(rate: u32) -> Self {
        Self::with_gate(rate, Box::new(EnergyGate::new()))
    }

    /// Create a segmenter with a custom speech gate (e.g. a neural VAD), keeping
    /// the same pre-roll / hangover / length buffering.
    pub fn with_gate(rate: u32, gate: Box<dyn SpeechGate>) -> Self {
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
            gate,
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
        self.gate.level()
    }

    /// Whether a speech run is currently in progress. Drives the dictation
    /// inactivity timeout (a long pause with no speech ends the session).
    pub fn is_active(&self) -> bool {
        self.speaking
    }

    /// The in-progress utterance buffer (mono samples at the capture rate) — the
    /// audio accumulated since speech onset, before a pause finalizes it. Used to
    /// transcribe partial results for live streaming dictation. Empty when not
    /// currently speaking.
    pub fn utterance(&self) -> &[f32] {
        &self.utterance
    }

    /// Feed one captured buffer. Returns a finalized utterance (mono samples at
    /// the capture rate) when a speech run just ended, otherwise `None`.
    pub fn push(&mut self, frame: &[f32]) -> Option<Vec<f32>> {
        if frame.is_empty() {
            return None;
        }
        if self.gate.is_speech(frame, self.rate) {
            if !self.speaking {
                self.speaking = true;
                self.utterance.clear();
                self.utterance.extend(self.preroll.drain(..));
            }
            self.utterance.extend_from_slice(frame);
            self.speech_samples += frame.len();
            self.silence_run = 0;
        } else if self.speaking {
            self.utterance.extend_from_slice(frame);
            self.silence_run += frame.len();
        } else {
            self.preroll.extend(frame.iter().copied());
            while self.preroll.len() > self.preroll_cap {
                self.preroll.pop_front();
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
