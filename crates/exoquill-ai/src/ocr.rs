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

/// One recognized word with its bounding box, in the pixel space of the image
/// the OCR engine actually saw (see [`OcrLayout::width`]/[`height`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrWord {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
}

/// A structured OCR result: layout-preserving text plus per-word boxes for a
/// selectable overlay. `width`/`height` are the dimensions the boxes live in
/// (the preprocessed image), so the UI can scale them to the displayed image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLayout {
    pub text: String,
    pub words: Vec<OcrWord>,
    pub width: u32,
    pub height: u32,
}

/// Extracts text from an image.
pub trait OcrProvider: Provider {
    fn run(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrResponse>;

    /// Recognize text *with* per-word bounding boxes and layout-preserving text
    /// (for the selectable OCR overlay). The default falls back to plain
    /// [`run`](OcrProvider::run) with no boxes, so providers without layout
    /// support still work — the overlay simply shows the text.
    fn run_layout(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrLayout> {
        let response = self.run(request, cancel)?;
        Ok(OcrLayout {
            text: response.text,
            words: Vec::new(),
            width: 0,
            height: 0,
        })
    }
}
