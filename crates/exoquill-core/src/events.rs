//! The backend event bus (product spec §17.2).

use serde::Serialize;

use crate::jobs::Job;

/// Events the backend emits toward the frontend. Serialized with a `type` tag
/// so the UI can switch on the variant.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A job changed state (queued → running → completed/failed/cancelled).
    JobUpdated { job: Job },
    /// A running job reported progress in `[0.0, 1.0]`.
    JobProgress { id: String, progress: f32 },
    /// The notes list changed and should be reloaded.
    NotesChanged,
    /// A non-fatal error to surface to the user.
    Error { message: String },
}

/// A sink the platform layer implements to deliver events. The Tauri layer
/// emits them to the webview; tests collect them.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}
