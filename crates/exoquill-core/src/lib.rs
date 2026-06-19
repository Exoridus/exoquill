//! Core domain logic for ExoQuill.
//!
//! This crate will host the note model, settings, the job queue and the event
//! bus (see `docs/roadmap.md`, PR 1 & PR 2). For now it only exposes a version
//! helper so the workspace wiring can be verified end to end.

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
