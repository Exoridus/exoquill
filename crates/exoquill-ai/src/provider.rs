//! Common provider metadata and health primitives.
//!
//! Every AI feature (STT, OCR, formatting, TTS, VAD) is exposed through a
//! provider that implements [`Provider`] plus its feature-specific trait. Runs
//! are synchronous and cooperatively cancellable via [`CancelToken`]; the job
//! queue executes them off the UI thread and heavy runtimes run as isolated
//! processes (decisions D8).

use std::fmt;
use std::process::Command;

use serde::{Deserialize, Serialize};

// The cancellation primitive lives in core so the job queue and providers share
// one type. Re-exported here for ergonomic `crate::provider::CancelToken` use.
pub use exoquill_core::CancelToken;

/// On Windows, start a child process at below-normal priority so heavy CPU work
/// (LLM inference, neural-TTS warm-up/synthesis) yields to the foreground UI and
/// doesn't freeze the webview. No-op on other platforms. Shared by the spawned
/// runtimes (llama-server, XTTS, Zonos).
pub(crate) fn below_normal_priority(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        command.creation_flags(BELOW_NORMAL_PRIORITY_CLASS);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// A capability a provider advertises to the UI and scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub key: String,
    pub description: String,
}

/// A model file a provider needs in order to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequirement {
    pub model_id: String,
    pub feature: String,
    pub required: bool,
}

/// License and provenance shown in the model manager / about screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    pub runtime_license: String,
    pub source: Option<String>,
}

/// Health of a provider and its runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Health {
    Ready,
    MissingModel { model_id: String },
    Unavailable { reason: String },
}

/// Errors a provider run can produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "message")]
pub enum ProviderError {
    /// The run was cancelled cooperatively.
    Cancelled,
    /// A required model was not installed.
    MissingModel(String),
    /// The underlying runtime failed (crash, bad output, …).
    Runtime(String),
    /// The request was malformed.
    InvalidInput(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Cancelled => write!(f, "run cancelled"),
            ProviderError::MissingModel(id) => write!(f, "missing model: {id}"),
            ProviderError::Runtime(msg) => write!(f, "runtime error: {msg}"),
            ProviderError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Result type for provider runs.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Metadata and lifecycle shared by every provider (product spec §14.1).
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn version(&self) -> &str;
    fn capabilities(&self) -> Vec<Capability>;
    fn required_models(&self) -> Vec<ModelRequirement>;
    fn license_info(&self) -> LicenseInfo;
    fn health_check(&self) -> Health;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_displays() {
        assert_eq!(ProviderError::Cancelled.to_string(), "run cancelled");
        assert_eq!(
            ProviderError::MissingModel("stt.whisper".into()).to_string(),
            "missing model: stt.whisper"
        );
    }
}
