//! AI provider interfaces for ExoQuill.
//!
//! Defines the provider traits (`SpeechToText`, `Ocr`, `Formatter`,
//! `TextToSpeech`, `Vad`) and deterministic mock implementations. Runs are
//! synchronous and cooperatively cancellable so the job queue can execute them
//! off the UI thread; heavy runtimes run as isolated processes (decisions D8).

pub mod chatterbox;
pub mod formatter;
pub mod kokoro;
pub mod llama;
pub mod llama_server;
pub mod mock;
pub mod ocr;
pub mod piper;
pub mod provider;
pub mod stt;
pub mod tesseract;
pub mod tts;
pub mod vad;
pub mod whisper;
pub mod whisper_server;
pub mod xtts;
pub mod zonos;

pub use chatterbox::{ChatterboxServer, ChatterboxTts};
pub use kokoro::{KokoroServer, KokoroTts};
pub use llama::LlamaFormatter;
pub use llama_server::{LlamaServer, LlamaServerFormatter};
pub use piper::PiperTts;
pub use tesseract::TesseractOcr;
pub use whisper::WhisperStt;
pub use whisper_server::{WhisperServer, WhisperServerStt};
pub use xtts::{XttsServer, XttsTts};
pub use zonos::{ZonosServer, ZonosTts};

pub use provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
