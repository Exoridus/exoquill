//! Speech-to-text provider (product spec §10).

use serde::{Deserialize, Serialize};

use crate::provider::{CancelToken, Provider, ProviderResult};

/// A chunk of audio to transcribe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttRequest {
    /// Mono PCM samples normalized to `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// `de_en_terms | en | auto`.
    pub language_mode: String,
    pub custom_terms: Vec<String>,
}

/// A transcription result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttResponse {
    pub text: String,
    pub language: Option<String>,
    pub confidence: Option<f32>,
}

/// Transcribes audio chunks to text.
pub trait SpeechToTextProvider: Provider {
    fn run(&self, request: SttRequest, cancel: &CancelToken) -> ProviderResult<SttResponse>;
}
