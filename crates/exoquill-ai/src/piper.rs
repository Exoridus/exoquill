//! Real text-to-speech provider backed by the Piper CLI (sidecar process).
//!
//! Text is written to Piper over stdin; raw 16-bit mono PCM comes back on
//! stdout (`--output-raw`) and is normalized to `f32` samples for playback.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse};

/// Neural TTS via a bundled Piper executable + ONNX voice.
pub struct PiperTts {
    binary: PathBuf,
    model: PathBuf,
    sample_rate: u32,
}

impl PiperTts {
    pub fn new(binary: impl Into<PathBuf>, model: impl Into<PathBuf>, sample_rate: u32) -> Self {
        Self {
            binary: binary.into(),
            model: model.into(),
            sample_rate,
        }
    }
}

impl Provider for PiperTts {
    fn id(&self) -> &str {
        "tts.piper"
    }
    fn display_name(&self) -> &str {
        "Piper TTS"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.piper.de".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "GPL-3.0".into(),
            source: Some("rhasspy/piper".into()),
        }
    }
    fn health_check(&self) -> Health {
        if !self.model.exists() {
            Health::MissingModel {
                model_id: "tts.piper.de".into(),
            }
        } else if self.binary.exists() {
            Health::Ready
        } else {
            Health::Unavailable {
                reason: format!("piper not found at {:?}", self.binary),
            }
        }
    }
}

impl TextToSpeechProvider for PiperTts {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: self.sample_rate,
            });
        }

        let mut child = Command::new(&self.binary)
            .arg("--model")
            .arg(&self.model)
            .arg("--output-raw")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn piper: {e}")))?;

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| ProviderError::Runtime("no stdin handle".into()))?;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| ProviderError::Runtime(format!("write text: {e}")))?;
        } // dropping stdin signals EOF so Piper synthesizes and exits

        let output = child
            .wait_with_output()
            .map_err(|e| ProviderError::Runtime(format!("piper wait: {e}")))?;
        if !output.status.success() {
            return Err(ProviderError::Runtime("piper synthesis failed".into()));
        }

        // Raw 16-bit little-endian mono PCM → normalized f32.
        let samples = output
            .stdout
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        Ok(TtsResponse {
            samples,
            sample_rate: self.sample_rate,
        })
    }
}
