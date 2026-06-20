//! Neural voice-activity detection via the Silero VAD ONNX model.
//!
//! Wraps Silero v5 behind the [`SpeechGate`] trait so the [`Segmenter`] can swap
//! it in for the energy gate in noisy rooms. ONNX Runtime is linked dynamically
//! (`ort`'s `load-dynamic`): the crate builds without the native library present,
//! and [`SileroGate::new`] fails cleanly when the runtime or model is missing so
//! the caller can fall back to the energy gate.
//!
//! NOTE: the inference path follows the documented Silero v5 interface (inputs
//! `input` `[1, 512]` f32, `state` `[2, 1, 128]` f32, `sr` i64 scalar; outputs
//! `output` and `stateN`). It needs `onnxruntime` + the model to exercise and is
//! not covered by the unit tests in this crate.
//!
//! [`Segmenter`]: crate::segmenter::Segmenter

use std::path::Path;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use crate::segmenter::SpeechGate;
use crate::{resample_to_16k, rms_level};

/// Silero v5 runs on 16 kHz audio in fixed 512-sample (32 ms) windows.
const SILERO_RATE: u32 = 16_000;
const WINDOW: usize = 512;
/// Recurrent state tensor, shape `[2, 1, 128]`.
const STATE_LEN: usize = 2 * 128;
/// Hysteresis: enter speech above `ENTER`, stay until below `EXIT`. The gap
/// debounces brief dips so a word isn't chopped mid-vowel.
const ENTER: f32 = 0.5;
const EXIT: f32 = 0.35;

/// A [`SpeechGate`] backed by the Silero VAD ONNX model.
pub struct SileroGate {
    session: Session,
    /// Recurrent state carried between windows (`[2, 1, 128]` flattened).
    state: Vec<f32>,
    /// 16 kHz samples awaiting a full window (resampled from the capture rate).
    buf: Vec<f32>,
    speaking: bool,
    last_level: f32,
}

impl SileroGate {
    /// Load the Silero model at `model_path`. Fails when ONNX Runtime can't be
    /// loaded (missing `onnxruntime` dylib) or the model can't be read, letting
    /// the caller fall back to the energy gate.
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("ort session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("ort optimization level: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("ort intra threads: {e}"))?
            .commit_from_file(model_path.as_ref())
            .map_err(|e| format!("load silero model: {e}"))?;
        Ok(Self {
            session,
            state: vec![0.0; STATE_LEN],
            buf: Vec::with_capacity(WINDOW * 2),
            speaking: false,
            last_level: 0.0,
        })
    }

    /// Run the model on one 512-sample window, updating the recurrent state and
    /// returning the speech probability in `[0, 1]`.
    fn infer(&mut self, window: &[f32]) -> Result<f32, String> {
        let input = Tensor::from_array((vec![1_i64, WINDOW as i64], window.to_vec()))
            .map_err(|e| format!("silero input tensor: {e}"))?;
        let state = Tensor::from_array((vec![2_i64, 1, 128], self.state.clone()))
            .map_err(|e| format!("silero state tensor: {e}"))?;
        // `sr` is a rank-0 (scalar) i64 tensor: an empty shape with one element.
        let sr = Tensor::from_array((Vec::<i64>::new(), vec![SILERO_RATE as i64]))
            .map_err(|e| format!("silero sr tensor: {e}"))?;

        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "state" => state, "sr" => sr])
            .map_err(|e| format!("silero run: {e}"))?;

        let prob = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("silero output: {e}"))?
            .1
            .first()
            .copied()
            .unwrap_or(0.0);
        let new_state = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("silero state out: {e}"))?
            .1
            .to_vec();
        if new_state.len() == STATE_LEN {
            self.state = new_state;
        }
        Ok(prob)
    }
}

impl SpeechGate for SileroGate {
    fn is_speech(&mut self, frame: &[f32], rate: u32) -> bool {
        if frame.is_empty() {
            return self.speaking;
        }
        self.last_level = rms_level(frame);
        // Resample to Silero's 16 kHz and accumulate into fixed windows.
        self.buf.extend_from_slice(&resample_to_16k(frame, rate));
        while self.buf.len() >= WINDOW {
            let window: Vec<f32> = self.buf.drain(..WINDOW).collect();
            match self.infer(&window) {
                Ok(prob) => {
                    self.speaking = if self.speaking {
                        prob >= EXIT
                    } else {
                        prob >= ENTER
                    };
                }
                // A transient inference failure must not wedge capture; hold the
                // current decision and keep going.
                Err(err) => eprintln!("silero inference error: {err}"),
            }
        }
        self.speaking
    }

    fn level(&self) -> f32 {
        self.last_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model path from the env, only if the file is actually present — lets
    /// this test exercise real inference when the assets were fetched
    /// (`scripts/fetch-silero.ps1` + `ORT_DYLIB_PATH`), and skip otherwise.
    fn model_path() -> Option<std::path::PathBuf> {
        std::env::var("EXOQUILL_SILERO_MODEL")
            .ok()
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
    }

    #[test]
    fn runs_real_inference_when_model_present() {
        let Some(model) = model_path() else {
            eprintln!("skipping: EXOQUILL_SILERO_MODEL not set/present");
            return;
        };
        let mut gate = SileroGate::new(&model).expect("load silero model");

        // Inference on a silent window must succeed (proves tensor shapes, the
        // scalar `sr`, and the input/output names are all correct) and yield a
        // low speech probability.
        let p_silence = gate.infer(&[0.0_f32; WINDOW]).expect("inference failed");
        assert!((0.0..=1.0).contains(&p_silence), "prob out of range: {p_silence}");
        assert!(p_silence < 0.5, "silence scored as speech: {p_silence}");

        // A non-silent window must also run and stay in range (state carried over).
        let buzz: Vec<f32> = (0..WINDOW)
            .map(|i| if i % 2 == 0 { 0.3 } else { -0.3 })
            .collect();
        let p_buzz = gate.infer(&buzz).expect("inference failed");
        assert!((0.0..=1.0).contains(&p_buzz), "prob out of range: {p_buzz}");
    }

    #[test]
    fn is_speech_segments_silence_when_model_present() {
        let Some(model) = model_path() else {
            eprintln!("skipping: EXOQUILL_SILERO_MODEL not set/present");
            return;
        };
        let mut gate = SileroGate::new(&model).expect("load silero model");
        // One second of silence at 16 kHz, fed in capture-sized chunks.
        let mut spoke = false;
        for chunk in [0.0_f32; 16_000].chunks(1600) {
            spoke |= gate.is_speech(chunk, 16_000);
        }
        assert!(!spoke, "silence classified as speech");
    }
}
