//! Core domain logic for ExoQuill.
//!
//! Hosts the note model, title generation, cancellation, the event bus and the
//! job queue. Persistence lives in `exoquill-db`; AI providers in `exoquill-ai`.

pub mod cancel;
pub mod clock;
pub mod events;
pub mod jobs;
pub mod note;

pub use cancel::CancelToken;
pub use events::{Event, EventSink};
pub use jobs::{Job, JobHandle, JobId, JobQueue, JobStatus, JobTask};
pub use note::{
    generate_title, new_note_id, NewNote, Note, NoteSource, NoteUpdate, DEFAULT_LANGUAGE_MODE,
};

/// Returns the ExoQuill core crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
