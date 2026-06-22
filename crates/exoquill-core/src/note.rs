//! Note domain model and title generation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Default language mode for new notes (German with English technical terms).
pub const DEFAULT_LANGUAGE_MODE: &str = "de_en_terms";

/// Maximum length of a title auto-derived from note content.
const MAX_TITLE_LEN: usize = 80;

/// How a note (or the text written into it) originated. Drives auto-titles and,
/// later, source badges in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteSource {
    #[default]
    Manual,
    Dictation,
    Ocr,
}

/// Which slice of notes a listing returns. Backs the sidebar's scope tabs:
/// `Active` (live, un-archived) is the default; `Archived` and `Trash` are the
/// other two views. A note is in exactly one scope at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteScope {
    #[default]
    Active,
    Archived,
    Trash,
}

impl NoteScope {
    /// The SQL predicate selecting this scope's rows. A fixed string per variant
    /// (no user input), so it's safe to interpolate into a query.
    pub fn predicate(self) -> &'static str {
        match self {
            NoteScope::Active => "deleted_at IS NULL AND archived = 0",
            NoteScope::Archived => "deleted_at IS NULL AND archived = 1",
            NoteScope::Trash => "deleted_at IS NOT NULL",
        }
    }
}

/// Sort order for a note listing, applied within the pinned/un-pinned split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteSort {
    #[default]
    Modified,
    Created,
    Title,
}

impl NoteSort {
    /// The SQL `ORDER BY` term for this sort (a fixed string per variant).
    pub fn order_by(self) -> &'static str {
        match self {
            NoteSort::Modified => "updated_at DESC",
            NoteSort::Created => "created_at DESC",
            NoteSort::Title => "title COLLATE NOCASE ASC",
        }
    }
}

/// A note as persisted and sent to the frontend (camelCase over the IPC bridge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    /// `true` while the title is auto-derived from the content (the user hasn't
    /// edited it). Cleared the moment the user types a title; restored when they
    /// clear it again. Drives the auto-title regeneration in the persistence layer.
    pub title_auto: bool,
    pub content_markdown: String,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
    pub archived: bool,
    pub deleted_at: Option<String>,
    pub language_mode: String,
    pub last_cursor_position: i64,
}

/// Input for creating a note. An empty `NewNote::default()` is what the
/// auto-create resolver uses when a tool runs without an active note.
#[derive(Debug, Clone, Default)]
pub struct NewNote {
    /// Explicit title; when `None` the title is derived from `content_markdown`.
    pub title: Option<String>,
    pub content_markdown: String,
    pub source: NoteSource,
    pub language_mode: Option<String>,
}

/// A recorded note event: the audit trail and undo safety net for an operation
/// that wrote into a note (formatting, OCR, dictation). `raw_text` keeps the
/// pre-operation text (product spec D6). Sent to the frontend for the history
/// view (camelCase over the IPC bridge).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteEvent {
    pub id: String,
    pub note_id: String,
    pub source_type: String,
    pub raw_text: Option<String>,
    pub processed_text: Option<String>,
    pub operation: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub model_version: Option<String>,
    pub created_at: String,
}

/// Input for recording a [`NoteEvent`]; `id` and `created_at` are filled in by
/// the persistence layer.
#[derive(Debug, Clone, Default)]
pub struct NewNoteEvent {
    pub note_id: String,
    pub source_type: String,
    pub raw_text: Option<String>,
    pub processed_text: Option<String>,
    pub operation: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub model_version: Option<String>,
}

/// A stored snapshot of a note's content for the edit-history diff timeline
/// (sent to the frontend as camelCase). Snapshots are deduplicated by
/// `content_hash`, so a no-op save adds nothing. `source` is `"manual"` (a
/// typing-pause snapshot) or `"op"` (written by an operation — format/OCR/
/// dictation); `op` names that operation when `source == "op"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteVersion {
    pub id: String,
    pub note_id: String,
    pub created_at: String,
    pub content_markdown: String,
    pub content_hash: String,
    pub source: String,
    pub op: Option<String>,
    pub provider_id: Option<String>,
}

/// Input for recording a [`NoteVersion`]; `id`, `created_at`, and `content_hash`
/// are filled in by the persistence layer.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewNoteVersion {
    pub note_id: String,
    pub content_markdown: String,
    /// `"manual"` | `"op"`; defaults to `"manual"` when omitted by the caller.
    #[serde(default)]
    pub source: Option<String>,
    pub op: Option<String>,
    pub provider_id: Option<String>,
}

/// Partial update for a note. Only `Some` fields are written; `updated_at` is
/// always bumped by the persistence layer.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteUpdate {
    pub title: Option<String>,
    pub content_markdown: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    pub language_mode: Option<String>,
    pub last_cursor_position: Option<i64>,
}

/// Generate a fresh, unique note id.
pub fn new_note_id() -> String {
    Uuid::new_v4().to_string()
}

/// Derive a note title from its content and origin.
///
/// Uses the first non-empty line (stripped of Markdown heading markers and
/// truncated) when present; otherwise falls back to a source-specific default.
/// `timestamp` is a pre-formatted `YYYY-MM-DD HH:mm` string used only for the
/// OCR/dictation fallbacks (see product spec §8.2).
pub fn generate_title(content: &str, source: NoteSource, timestamp: &str) -> String {
    if let Some(line) = first_meaningful_line(content) {
        return line;
    }
    match source {
        NoteSource::Ocr => format!("OCR Note – {timestamp}"),
        NoteSource::Dictation => format!("Dictation – {timestamp}"),
        NoteSource::Manual => "Untitled Note".to_string(),
    }
}

fn first_meaningful_line(content: &str) -> Option<String> {
    content
        .lines()
        .map(|line| line.trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .map(truncate_title)
}

fn truncate_title(line: &str) -> String {
    if line.chars().count() <= MAX_TITLE_LEN {
        line.to_string()
    } else {
        let head: String = line.chars().take(MAX_TITLE_LEN - 1).collect();
        format!("{}…", head.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_uses_first_meaningful_line() {
        let title = generate_title(
            "\n\n  Meeting notes  \nmore",
            NoteSource::Manual,
            "2026-06-19 10:00",
        );
        assert_eq!(title, "Meeting notes");
    }

    #[test]
    fn title_strips_markdown_heading() {
        let title = generate_title("# Projektidee", NoteSource::Manual, "2026-06-19 10:00");
        assert_eq!(title, "Projektidee");
    }

    #[test]
    fn empty_manual_note_is_untitled() {
        assert_eq!(
            generate_title("   \n\n", NoteSource::Manual, "2026-06-19 10:00"),
            "Untitled Note"
        );
    }

    #[test]
    fn empty_ocr_and_dictation_use_timestamp() {
        assert_eq!(
            generate_title("", NoteSource::Ocr, "2026-06-19 10:00"),
            "OCR Note – 2026-06-19 10:00"
        );
        assert_eq!(
            generate_title("", NoteSource::Dictation, "2026-06-19 10:00"),
            "Dictation – 2026-06-19 10:00"
        );
    }

    #[test]
    fn long_first_line_is_truncated() {
        let long = "x".repeat(200);
        let title = generate_title(&long, NoteSource::Manual, "2026-06-19 10:00");
        assert!(title.chars().count() <= MAX_TITLE_LEN);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_note_id(), new_note_id());
    }
}
