//! AI provider interfaces for ExoQuill.
//!
//! Will define the `SpeechToTextProvider`, `OcrProvider`, `FormatterProvider`,
//! `TextToSpeechProvider` and `VadProvider` traits and their mock + real
//! implementations (see `docs/roadmap.md`, PR 2 onward). Heavy runtimes run as
//! isolated processes (`docs/decisions.md`, D8).
