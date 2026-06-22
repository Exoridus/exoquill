//! In-memory mock providers for development and tests (PR 2). No real models,
//! deterministic output, used to build the job queue and UI before real
//! runtimes land.

use crate::formatter::{FormatRequest, FormatResponse, FormatterProvider};
use crate::ocr::{OcrProvider, OcrRequest, OcrResponse};
use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
use crate::stt::{SpeechToTextProvider, SttRequest, SttResponse};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse};
use crate::vad::VadProvider;

fn bail_if_cancelled(cancel: &CancelToken) -> ProviderResult<()> {
    if cancel.is_cancelled() {
        Err(ProviderError::Cancelled)
    } else {
        Ok(())
    }
}

/// Generate a trivial [`Provider`] metadata impl for a mock type.
macro_rules! mock_meta {
    ($ty:ty, $id:literal, $name:literal) => {
        impl Provider for $ty {
            fn id(&self) -> &str {
                $id
            }
            fn display_name(&self) -> &str {
                $name
            }
            fn version(&self) -> &str {
                "0.0.0"
            }
            fn capabilities(&self) -> Vec<Capability> {
                Vec::new()
            }
            fn required_models(&self) -> Vec<ModelRequirement> {
                Vec::new()
            }
            fn license_info(&self) -> LicenseInfo {
                LicenseInfo {
                    runtime_license: "MIT".into(),
                    source: None,
                }
            }
            fn health_check(&self) -> Health {
                Health::Ready
            }
        }
    };
}

/// Formatter mock: whitespace-normalizes the input without inventing content.
#[derive(Debug, Default)]
pub struct MockFormatter;
mock_meta!(MockFormatter, "formatter.mock", "Mock Formatter");

impl FormatterProvider for MockFormatter {
    fn run(&self, request: FormatRequest, cancel: &CancelToken) -> ProviderResult<FormatResponse> {
        bail_if_cancelled(cancel)?;
        // Collapse intra-line whitespace but keep paragraph/line structure.
        let formatted = request
            .text
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        Ok(FormatResponse {
            formatted_text: formatted,
            warnings: Vec::new(),
            changed_meaning_risk: "low".into(),
        })
    }
}

/// STT mock: returns a placeholder transcript.
#[derive(Debug, Default)]
pub struct MockSpeechToText;
mock_meta!(MockSpeechToText, "stt.mock", "Mock Speech-to-Text");

impl SpeechToTextProvider for MockSpeechToText {
    fn run(&self, request: SttRequest, cancel: &CancelToken) -> ProviderResult<SttResponse> {
        bail_if_cancelled(cancel)?;
        if request.samples.is_empty() {
            return Err(ProviderError::InvalidInput("no audio samples".into()));
        }
        Ok(SttResponse {
            text: format!("[mock transcript of {} samples]", request.samples.len()),
            language: Some("de".into()),
            confidence: Some(0.9),
        })
    }
}

/// OCR mock: returns placeholder text, rejects empty images.
#[derive(Debug, Default)]
pub struct MockOcr;
mock_meta!(MockOcr, "ocr.mock", "Mock OCR");

impl OcrProvider for MockOcr {
    fn run(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrResponse> {
        bail_if_cancelled(cancel)?;
        if request.image_bytes.is_empty() {
            return Err(ProviderError::InvalidInput("empty image".into()));
        }
        Ok(OcrResponse {
            text: "[mock ocr text]".into(),
            confidence: Some(0.85),
        })
    }
}

/// TTS mock: emits silence, one sample per character, to exercise the queue.
#[derive(Debug, Default)]
pub struct MockTextToSpeech;
mock_meta!(MockTextToSpeech, "tts.mock", "Mock Text-to-Speech");

impl TextToSpeechProvider for MockTextToSpeech {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse> {
        bail_if_cancelled(cancel)?;
        let samples = vec![0.0_f32; request.text.chars().count().max(1)];
        Ok(TtsResponse {
            samples,
            sample_rate: 22_050,
        })
    }
}

/// VAD mock: derives speech probability from frame RMS energy.
#[derive(Debug, Default)]
pub struct MockVad;
mock_meta!(MockVad, "vad.mock", "Mock VAD");

impl VadProvider for MockVad {
    fn detect(&self, frame: &[f32], _sample_rate: u32) -> ProviderResult<f32> {
        if frame.is_empty() {
            return Ok(0.0);
        }
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        Ok((rms * 4.0).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_collapses_whitespace() {
        let out = MockFormatter
            .run(
                FormatRequest {
                    text: "  hallo   welt \n ".into(),
                    source: "manual".into(),
                    language_mode: "de_en_terms".into(),
                    operation: "quick_format".into(),
                    instruction: None,
                    custom_terms: Vec::new(),
                },
                &CancelToken::new(),
            )
            .unwrap();
        assert_eq!(out.formatted_text, "hallo welt");
        assert_eq!(out.changed_meaning_risk, "low");
    }

    #[test]
    fn cancelled_run_returns_cancelled() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = MockOcr
            .run(
                OcrRequest {
                    image_bytes: vec![1, 2, 3],
                    languages: "deu+eng".into(),
                },
                &cancel,
            )
            .unwrap_err();
        assert_eq!(err, ProviderError::Cancelled);
    }

    #[test]
    fn ocr_rejects_empty_image() {
        let err = MockOcr
            .run(
                OcrRequest {
                    image_bytes: Vec::new(),
                    languages: "deu+eng".into(),
                },
                &CancelToken::new(),
            )
            .unwrap_err();
        assert!(matches!(err, ProviderError::InvalidInput(_)));
    }

    #[test]
    fn vad_scores_louder_frames_higher() {
        let silence = vec![0.0_f32; 256];
        let loud = vec![0.5_f32; 256];
        let quiet_score = MockVad.detect(&silence, 16_000).unwrap();
        let loud_score = MockVad.detect(&loud, 16_000).unwrap();
        assert!(loud_score > quiet_score);
        assert!((0.0..=1.0).contains(&loud_score));
    }

    #[test]
    fn tts_emits_samples() {
        let out = MockTextToSpeech
            .run(
                TtsRequest::new("hallo", "de-calm", 1.0),
                &CancelToken::new(),
            )
            .unwrap();
        assert_eq!(out.samples.len(), 5);
        assert_eq!(out.sample_rate, 22_050);
    }

    #[test]
    fn providers_are_object_safe() {
        // The job queue stores providers as trait objects; ensure that compiles.
        let _f: Box<dyn FormatterProvider> = Box::new(MockFormatter);
        let _s: Box<dyn SpeechToTextProvider> = Box::new(MockSpeechToText);
        let _o: Box<dyn OcrProvider> = Box::new(MockOcr);
        let _t: Box<dyn TextToSpeechProvider> = Box::new(MockTextToSpeech);
        let _v: Box<dyn VadProvider> = Box::new(MockVad);
    }
}
