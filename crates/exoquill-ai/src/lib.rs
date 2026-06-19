//! AI provider interfaces for ExoQuill.
//!
//! Defines the provider traits (`SpeechToText`, `Ocr`, `Formatter`,
//! `TextToSpeech`, `Vad`) and deterministic mock implementations. Runs are
//! synchronous and cooperatively cancellable so the job queue can execute them
//! off the UI thread; heavy runtimes run as isolated processes (decisions D8).

pub mod formatter;
pub mod mock;
pub mod ocr;
pub mod provider;
pub mod stt;
pub mod tts;
pub mod vad;

pub use provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
