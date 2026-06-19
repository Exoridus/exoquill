//! Core domain logic for ExoQuill.
//!
//! Hosts the note model and title generation today; the job queue and event bus
//! land in PR 2 (see `docs/roadmap.md`). Persistence lives in `exoquill-db`.

pub mod clock;
pub mod note;

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
