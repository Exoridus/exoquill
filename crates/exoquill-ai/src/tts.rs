//! Text-to-speech provider (product spec §13).

use serde::{Deserialize, Serialize};

use crate::provider::{CancelToken, Provider, ProviderResult};

/// A segment of text to synthesize.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: String,
    pub speed: f32,
}

/// Synthesized audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsResponse {
    /// Mono PCM samples normalized to `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Synthesizes speech from text segments (the read-aloud queue calls this per
/// sentence, product spec §13.5).
pub trait TextToSpeechProvider: Provider {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse>;
}
