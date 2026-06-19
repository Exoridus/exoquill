//! Formatter provider: controlled cleanup of rough text (product spec §12).

use serde::{Deserialize, Serialize};

use crate::provider::{CancelToken, Provider, ProviderResult};

/// Structured formatter input (product spec §12.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatRequest {
    pub text: String,
    /// `dictation | ocr | manual | mixed`.
    pub source: String,
    /// `de_en_terms | en | auto`.
    pub language_mode: String,
    /// `quick_format | custom_format`.
    pub operation: String,
    pub instruction: Option<String>,
    pub custom_terms: Vec<String>,
}

/// Structured formatter output (product spec §12.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatResponse {
    pub formatted_text: String,
    pub warnings: Vec<String>,
    /// `low | medium | high`.
    pub changed_meaning_risk: String,
}

/// Cleans rough dictation/OCR text into readable Markdown without inventing
/// content.
pub trait FormatterProvider: Provider {
    fn run(&self, request: FormatRequest, cancel: &CancelToken) -> ProviderResult<FormatResponse>;
}
