//! Image capture for ExoQuill.
//!
//! File picker, drag-and-drop and clipboard image intake for OCR; screen-region
//! capture lands in v0.2 (see `docs/decisions.md`, D4).
//!
//! This crate also hosts the [`preprocess`] pipeline that cleans up captured
//! images before they are handed to the OCR engine.

pub mod preprocess;

pub use preprocess::{preprocess_for_ocr, PreprocessError};
