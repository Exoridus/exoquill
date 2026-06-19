//! AI provider interfaces for ExoQuill.
//!
//! Defines the provider traits (`SpeechToText`, `Ocr`, `Formatter`,
//! `TextToSpeech`, `Vad`) and deterministic mock implementations. Runs are
//! synchronous and cooperatively cancellable so the job queue can execute them
//! off the UI thread; heavy runtimes run as isolated processes (decisions D8).

pub mod formatter;
pub mod llama;
pub mod mock;
pub mod ocr;
pub mod piper;
pub mod provider;
pub mod stt;
pub mod tesseract;
pub mod tts;
pub mod vad;

pub use llama::LlamaFormatter;
pub use piper::PiperTts;
pub use tesseract::TesseractOcr;

pub use provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
