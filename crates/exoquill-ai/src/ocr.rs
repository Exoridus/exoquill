//! OCR provider (product spec §11).

use serde::{Deserialize, Serialize};

use crate::provider::{CancelToken, Provider, ProviderResult};

/// An image to extract text from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRequest {
    /// Encoded image bytes (PNG, JPEG, …).
    pub image_bytes: Vec<u8>,
    /// Tesseract-style language code, e.g. `deu+eng`.
    pub languages: String,
}

/// An OCR result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResponse {
    pub text: String,
    pub confidence: Option<f32>,
}

/// Extracts text from an image.
pub trait OcrProvider: Provider {
    fn run(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrResponse>;
}
