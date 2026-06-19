//! SQLite persistence layer for ExoQuill.
//!
//! Owns the schema and the note repository. Operations return
//! [`rusqlite::Result`]; the Tauri command layer maps errors to strings.

use exoquill_core::clock::{now_rfc3339, title_timestamp};
use exoquill_core::note::{
    generate_title, new_note_id, NewNote, Note, NoteUpdate, DEFAULT_LANGUAGE_MODE,
};
use rusqlite::{params, Connection, OptionalExtension, Result, Row};

/// Schema version stamped into `PRAGMA user_version`. Bump when the schema
/// changes and add a migration step in [`Database::migrate`].
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    id                   TEXT PRIMARY KEY,
    title                TEXT NOT NULL,
    content_markdown     TEXT NOT NULL DEFAULT '',
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    pinned               INTEGER NOT NULL DEFAULT 0,
    archived             INTEGER NOT NULL DEFAULT 0,
    deleted_at           TEXT,
    language_mode        TEXT NOT NULL DEFAULT 'de_en_terms',
    last_cursor_position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_notes_updated_at ON notes (updated_at DESC);

CREATE TABLE IF NOT EXISTS note_events (
    id             TEXT PRIMARY KEY,
    note_id        TEXT NOT NULL,
    source_type    TEXT NOT NULL,
    raw_text       TEXT,
    processed_text TEXT,
    operation      TEXT,
    provider_id    TEXT,
    model_id       TEXT,
    model_version  TEXT,
    confidence_json TEXT,
    metadata_json  TEXT,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (note_id) REFERENCES notes (id)
);

CREATE INDEX IF NOT EXISTS idx_note_events_note_id ON note_events (note_id);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

/// A SQLite-backed ExoQuill database. Not `Sync`; wrap in a `Mutex` for shared
/// access (the Tauri layer manages it as state).
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) a database at `path` and apply migrations.
    pub fn open(path: &str) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    /// Open an in-memory database (used by tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Create a note. When `new.title` is `None`, the title is auto-derived.
    pub fn create_note(&self, new: NewNote) -> Result<Note> {
        let now = now_rfc3339();
        let language_mode = new
            .language_mode
            .unwrap_or_else(|| DEFAULT_LANGUAGE_MODE.to_string());
        let title = new.title.unwrap_or_else(|| {
            generate_title(&new.content_markdown, new.source, &title_timestamp())
        });

        let note = Note {
            id: new_note_id(),
            title,
            content_markdown: new.content_markdown,
            created_at: now.clone(),
            updated_at: now,
            pinned: false,
            archived: false,
            deleted_at: None,
            language_mode,
            last_cursor_position: 0,
        };

        self.conn.execute(
            "INSERT INTO notes \
             (id, title, content_markdown, created_at, updated_at, pinned, archived, deleted_at, language_mode, last_cursor_position) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                note.id,
                note.title,
                note.content_markdown,
                note.created_at,
                note.updated_at,
                note.pinned,
                note.archived,
                note.deleted_at,
                note.language_mode,
                note.last_cursor_position,
            ],
        )?;
        Ok(note)
    }

    /// Fetch a note by id, including soft-deleted ones.
    pub fn get_note(&self, id: &str) -> Result<Option<Note>> {
        self.conn
            .query_row(
                "SELECT * FROM notes WHERE id = ?1",
                params![id],
                row_to_note,
            )
            .optional()
    }

    /// Apply a partial update and bump `updated_at`. Returns `None` if no live
    /// note with that id exists.
    pub fn update_note(&self, id: &str, update: NoteUpdate) -> Result<Option<Note>> {
        let Some(mut note) = self.get_note(id)? else {
            return Ok(None);
        };
        if note.deleted_at.is_some() {
            return Ok(None);
        }

        if let Some(v) = update.title {
            note.title = v;
        }
        if let Some(v) = update.content_markdown {
            note.content_markdown = v;
        }
        if let Some(v) = update.pinned {
            note.pinned = v;
        }
        if let Some(v) = update.archived {
            note.archived = v;
        }
        if let Some(v) = update.language_mode {
            note.language_mode = v;
        }
        if let Some(v) = update.last_cursor_position {
            note.last_cursor_position = v;
        }
        note.updated_at = now_rfc3339();

        self.conn.execute(
            "UPDATE notes SET title = ?2, content_markdown = ?3, pinned = ?4, archived = ?5, \
             language_mode = ?6, last_cursor_position = ?7, updated_at = ?8 WHERE id = ?1",
            params![
                note.id,
                note.title,
                note.content_markdown,
                note.pinned,
                note.archived,
                note.language_mode,
                note.last_cursor_position,
                note.updated_at,
            ],
        )?;
        Ok(Some(note))
    }

    /// Soft-delete a note. Returns `true` if a live note was deleted.
    pub fn delete_note(&self, id: &str) -> Result<bool> {
        let now = now_rfc3339();
        let affected = self.conn.execute(
            "UPDATE notes SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        Ok(affected > 0)
    }

    /// List all live notes, pinned first, then most recently updated.
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM notes WHERE deleted_at IS NULL \
             ORDER BY pinned DESC, updated_at DESC",
        )?;
        let notes = stmt
            .query_map([], row_to_note)?
            .collect::<Result<Vec<_>>>()?;
        Ok(notes)
    }

    /// Basic case-insensitive search over title and content of live notes.
    pub fn search_notes(&self, query: &str) -> Result<Vec<Note>> {
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self.conn.prepare(
            "SELECT * FROM notes WHERE deleted_at IS NULL \
             AND (title LIKE ?1 ESCAPE '\\' OR content_markdown LIKE ?1 ESCAPE '\\') \
             ORDER BY pinned DESC, updated_at DESC",
        )?;
        let notes = stmt
            .query_map(params![pattern], row_to_note)?
            .collect::<Result<Vec<_>>>()?;
        Ok(notes)
    }

    /// Resolve the note a tool should write into: the active note if it exists
    /// and is live, otherwise a freshly created empty note (product spec §5.3).
    pub fn resolve_target_note(&self, active: Option<&str>) -> Result<Note> {
        if let Some(id) = active {
            if let Some(note) = self.get_note(id)? {
                if note.deleted_at.is_none() {
                    return Ok(note);
                }
            }
        }
        self.create_note(NewNote::default())
    }

    /// Read a setting value (raw JSON string).
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
    }

    /// Upsert a setting value (raw JSON string).
    pub fn set_setting(&self, key: &str, value_json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value_json = ?2, updated_at = ?3",
            params![key, value_json, now_rfc3339()],
        )?;
        Ok(())
    }
}

fn row_to_note(row: &Row) -> Result<Note> {
    Ok(Note {
        id: row.get("id")?,
        title: row.get("title")?,
        content_markdown: row.get("content_markdown")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        pinned: row.get("pinned")?,
        archived: row.get("archived")?,
        deleted_at: row.get("deleted_at")?,
        language_mode: row.get("language_mode")?,
        last_cursor_position: row.get("last_cursor_position")?,
    })
}

/// Escape LIKE wildcards in a user query so they match literally.
fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(content: &str) -> NewNote {
        NewNote {
            content_markdown: content.to_string(),
            ..Default::default()
        }
    }

    impl Database {
        /// Insert a note with an explicit `updated_at` so ordering tests are
        /// deterministic regardless of wall-clock resolution.
        fn insert_at(&self, id: &str, updated_at: &str, pinned: bool) {
            self.conn
                .execute(
                    "INSERT INTO notes (id, title, content_markdown, created_at, updated_at, \
                     pinned, archived, deleted_at, language_mode, last_cursor_position) \
                     VALUES (?1, 'x', '', ?2, ?2, ?3, 0, NULL, 'de_en_terms', 0)",
                    params![id, updated_at, pinned],
                )
                .unwrap();
        }
    }

    #[test]
    fn create_then_get_roundtrips() {
        let db = Database::open_in_memory().unwrap();
        let created = db.create_note(note("# Hallo Welt\nrest")).unwrap();
        assert_eq!(created.title, "Hallo Welt");
        let fetched = db.get_note(&created.id).unwrap().unwrap();
        assert_eq!(created, fetched);
    }

    #[test]
    fn update_changes_fields_and_bumps_updated_at() {
        let db = Database::open_in_memory().unwrap();
        let created = db.create_note(note("draft")).unwrap();
        let updated = db
            .update_note(
                &created.id,
                NoteUpdate {
                    content_markdown: Some("final".into()),
                    pinned: Some(true),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.content_markdown, "final");
        assert!(updated.pinned);
        assert!(updated.updated_at >= created.updated_at);
    }

    #[test]
    fn delete_is_soft_and_hides_from_list() {
        let db = Database::open_in_memory().unwrap();
        let created = db.create_note(note("bye")).unwrap();
        assert!(db.delete_note(&created.id).unwrap());
        assert!(db.list_notes().unwrap().is_empty());
        // Second delete is a no-op.
        assert!(!db.delete_note(&created.id).unwrap());
        // Row still exists (soft delete) but updates are rejected.
        assert!(db.get_note(&created.id).unwrap().is_some());
        assert!(db
            .update_note(&created.id, NoteUpdate::default())
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_orders_pinned_first_then_recent() {
        let db = Database::open_in_memory().unwrap();
        db.insert_at("a", "2026-06-19T10:00:00.000Z", false);
        db.insert_at("b", "2026-06-19T12:00:00.000Z", false);
        db.insert_at("c", "2026-06-19T09:00:00.000Z", true);
        let ids: Vec<String> = db.list_notes().unwrap().into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn search_matches_title_and_content_literally() {
        let db = Database::open_in_memory().unwrap();
        db.create_note(note("Rust und WebGPU")).unwrap();
        db.create_note(note("Einkaufsliste")).unwrap();
        assert_eq!(db.search_notes("webgpu").unwrap().len(), 1);
        // Wildcards are treated literally, not as SQL LIKE patterns.
        assert_eq!(db.search_notes("%").unwrap().len(), 0);
    }

    #[test]
    fn resolve_target_creates_when_missing_or_deleted() {
        let db = Database::open_in_memory().unwrap();
        // No active note -> new note.
        let fresh = db.resolve_target_note(None).unwrap();
        assert!(db.get_note(&fresh.id).unwrap().is_some());
        // Existing live note -> returned as-is.
        let same = db.resolve_target_note(Some(&fresh.id)).unwrap();
        assert_eq!(same.id, fresh.id);
        // Deleted active note -> new note.
        db.delete_note(&fresh.id).unwrap();
        let replacement = db.resolve_target_note(Some(&fresh.id)).unwrap();
        assert_ne!(replacement.id, fresh.id);
    }

    #[test]
    fn settings_upsert() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.get_setting("theme").unwrap().is_none());
        db.set_setting("theme", "\"dark\"").unwrap();
        db.set_setting("theme", "\"light\"").unwrap();
        assert_eq!(db.get_setting("theme").unwrap().unwrap(), "\"light\"");
    }
}
