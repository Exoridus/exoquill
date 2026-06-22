//! SQLite persistence layer for ExoQuill.
//!
//! Owns the schema and the note repository. Operations return
//! [`rusqlite::Result`]; the Tauri command layer maps errors to strings.

use exoquill_core::clock::{now_rfc3339, title_timestamp};
use exoquill_core::note::{
    generate_title, new_note_id, NewNote, NewNoteEvent, NewNoteVersion, Note, NoteEvent, NoteScope,
    NoteSort, NoteSource, NoteUpdate, NoteVersion, DEFAULT_LANGUAGE_MODE,
};
use rusqlite::{params, Connection, OptionalExtension, Result, Row};

/// Schema version stamped into `PRAGMA user_version`. Bump when the schema
/// changes and add a migration step in [`Database::migrate`].
const SCHEMA_VERSION: i64 = 3;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    id                   TEXT PRIMARY KEY,
    title                TEXT NOT NULL,
    title_auto           INTEGER NOT NULL DEFAULT 1,
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

CREATE TABLE IF NOT EXISTS note_versions (
    id           TEXT PRIMARY KEY,
    note_id      TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    content_md   TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    source       TEXT NOT NULL,
    op           TEXT,
    provider_id  TEXT,
    FOREIGN KEY (note_id) REFERENCES notes (id)
);

CREATE INDEX IF NOT EXISTS idx_note_versions_note_id ON note_versions (note_id);

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
        // WAL + NORMAL sync drastically cut write latency for the frequent
        // autosave writes (one per ~450 ms while typing) while staying durable
        // across app crashes (only the last in-flight txn risks an OS/power loss).
        // No-op on the in-memory test DB. Best-effort: never fail open on these.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        // v2: `title_auto` tracks whether a note's title is still auto-derived.
        // A fresh DB already has it (it's in SCHEMA); only older DBs need the
        // column added. Existing notes keep their titles (auto only for the ones
        // that clearly were never named).
        if !self.column_exists("notes", "title_auto")? {
            self.conn.execute_batch(
                "ALTER TABLE notes ADD COLUMN title_auto INTEGER NOT NULL DEFAULT 1; \
                 UPDATE notes SET title_auto = \
                 CASE WHEN title = '' OR title = 'Untitled Note' THEN 1 ELSE 0 END;",
            )?;
        }
        // v3: `note_versions` (edit-history snapshots) is created by the SCHEMA
        // batch above (CREATE TABLE IF NOT EXISTS), so older DBs pick it up on
        // open with no extra step — the version bump just records the change.
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Whether `table` has a column named `column` (used to make migrations
    /// idempotent regardless of the DB's prior schema version).
    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Create a note. When `new.title` is `None`, the title is auto-derived.
    pub fn create_note(&self, new: NewNote) -> Result<Note> {
        let now = now_rfc3339();
        let language_mode = new
            .language_mode
            .unwrap_or_else(|| DEFAULT_LANGUAGE_MODE.to_string());
        // An explicit title is the user's; a derived one stays auto-tracked so it
        // keeps following the content until the user names the note.
        let title_auto = new.title.is_none();
        let title = new.title.unwrap_or_else(|| {
            generate_title(&new.content_markdown, new.source, &title_timestamp())
        });

        let note = Note {
            id: new_note_id(),
            title,
            title_auto,
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
             (id, title, title_auto, content_markdown, created_at, updated_at, pinned, archived, deleted_at, language_mode, last_cursor_position) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                note.id,
                note.title,
                note.title_auto,
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
            .prepare_cached("SELECT * FROM notes WHERE id = ?1")?
            .query_row(params![id], row_to_note)
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
            // Naming the note pins the title; clearing it hands control back to
            // the auto-derivation below.
            note.title_auto = v.trim().is_empty();
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
        // Keep an un-named note's title in sync with its content (the first
        // meaningful line), so dictation/OCR/typing all surface a useful title.
        if note.title_auto {
            note.title =
                generate_title(&note.content_markdown, NoteSource::Manual, &title_timestamp());
        }
        note.updated_at = now_rfc3339();

        self.conn.prepare_cached(
            "UPDATE notes SET title = ?2, title_auto = ?3, content_markdown = ?4, pinned = ?5, \
             archived = ?6, language_mode = ?7, last_cursor_position = ?8, updated_at = ?9 \
             WHERE id = ?1",
        )?.execute(
            params![
                note.id,
                note.title,
                note.title_auto,
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

    /// Soft-delete (trash) a note. Returns `true` if a live note was trashed.
    pub fn delete_note(&self, id: &str) -> Result<bool> {
        let now = now_rfc3339();
        let affected = self.conn.execute(
            "UPDATE notes SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, now],
        )?;
        Ok(affected > 0)
    }

    /// Restore a trashed note (clears `deleted_at`). Returns `true` if a trashed
    /// note was restored.
    pub fn restore_note(&self, id: &str) -> Result<bool> {
        let now = now_rfc3339();
        let affected = self.conn.execute(
            "UPDATE notes SET deleted_at = NULL, updated_at = ?2 \
             WHERE id = ?1 AND deleted_at IS NOT NULL",
            params![id, now],
        )?;
        Ok(affected > 0)
    }

    /// Archive or un-archive a live note. Returns `true` if a live note changed.
    pub fn set_archived(&self, id: &str, archived: bool) -> Result<bool> {
        let now = now_rfc3339();
        let affected = self.conn.execute(
            "UPDATE notes SET archived = ?2, updated_at = ?3 \
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, archived, now],
        )?;
        Ok(affected > 0)
    }

    /// Permanently delete a note and its dependent rows (events + versions).
    /// Returns `true` if a note row was removed.
    pub fn hard_delete_note(&self, id: &str) -> Result<bool> {
        self.conn
            .execute("DELETE FROM note_versions WHERE note_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM note_events WHERE note_id = ?1", params![id])?;
        let affected = self
            .conn
            .execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Permanently delete trashed notes whose `deleted_at` is older than the
    /// given RFC-3339 cutoff (the trash-retention cleanup). Returns the count
    /// removed. RFC-3339 timestamps sort lexicographically, so a string compare
    /// is a valid time compare.
    pub fn purge_trash(&self, cutoff_rfc3339: &str) -> Result<usize> {
        let select = "SELECT id FROM notes WHERE deleted_at IS NOT NULL AND deleted_at < ?1";
        self.conn.execute(
            &format!("DELETE FROM note_versions WHERE note_id IN ({select})"),
            params![cutoff_rfc3339],
        )?;
        self.conn.execute(
            &format!("DELETE FROM note_events WHERE note_id IN ({select})"),
            params![cutoff_rfc3339],
        )?;
        let affected = self.conn.execute(
            "DELETE FROM notes WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
            params![cutoff_rfc3339],
        )?;
        Ok(affected)
    }

    /// List notes in `scope` (active / archived / trash), pinned first, then by
    /// `sort`. The caller's UI groups the pinned ones; we just keep them on top.
    pub fn list_notes(&self, scope: NoteScope, sort: NoteSort) -> Result<Vec<Note>> {
        let sql = format!(
            "SELECT * FROM notes WHERE {} ORDER BY pinned DESC, {}",
            scope.predicate(),
            sort.order_by(),
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let notes = stmt
            .query_map([], row_to_note)?
            .collect::<Result<Vec<_>>>()?;
        Ok(notes)
    }

    /// Case-insensitive search over title and content within `scope`, pinned
    /// first then most recently updated.
    pub fn search_notes(&self, query: &str, scope: NoteScope) -> Result<Vec<Note>> {
        let pattern = format!("%{}%", escape_like(query));
        let sql = format!(
            "SELECT * FROM notes WHERE {} \
             AND (title LIKE ?1 ESCAPE '\\' OR content_markdown LIKE ?1 ESCAPE '\\') \
             ORDER BY pinned DESC, updated_at DESC",
            scope.predicate(),
        );
        let mut stmt = self.conn.prepare(&sql)?;
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

    /// Record a note event (audit trail / undo safety net). Returns the stored
    /// event with its generated id and timestamp.
    pub fn insert_event(&self, new: NewNoteEvent) -> Result<NoteEvent> {
        let event = NoteEvent {
            id: new_note_id(),
            note_id: new.note_id,
            source_type: new.source_type,
            raw_text: new.raw_text,
            processed_text: new.processed_text,
            operation: new.operation,
            provider_id: new.provider_id,
            model_id: new.model_id,
            model_version: new.model_version,
            created_at: now_rfc3339(),
        };
        self.conn.execute(
            "INSERT INTO note_events \
             (id, note_id, source_type, raw_text, processed_text, operation, provider_id, \
              model_id, model_version, confidence_json, metadata_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10)",
            params![
                event.id,
                event.note_id,
                event.source_type,
                event.raw_text,
                event.processed_text,
                event.operation,
                event.provider_id,
                event.model_id,
                event.model_version,
                event.created_at,
            ],
        )?;
        Ok(event)
    }

    /// List a note's recorded events, most recent first. Ties on `created_at`
    /// (same-millisecond inserts) break by `rowid`, i.e. insertion order.
    pub fn list_events(&self, note_id: &str) -> Result<Vec<NoteEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, note_id, source_type, raw_text, processed_text, operation, provider_id, \
             model_id, model_version, created_at \
             FROM note_events WHERE note_id = ?1 ORDER BY created_at DESC, rowid DESC",
        )?;
        let events = stmt
            .query_map(params![note_id], row_to_event)?
            .collect::<Result<Vec<_>>>()?;
        Ok(events)
    }

    /// Record a content snapshot for the edit history, unless it's identical to
    /// the note's latest stored version (dedup by content hash → "only on real
    /// changes"). Returns the stored version, or `None` when it was a no-op
    /// duplicate. `source` defaults to `"manual"`.
    pub fn insert_version(&self, new: NewNoteVersion) -> Result<Option<NoteVersion>> {
        let hash = content_hash(&new.content_markdown);
        let latest: Option<String> = self
            .conn
            .query_row(
                "SELECT content_hash FROM note_versions WHERE note_id = ?1 \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                params![new.note_id],
                |r| r.get(0),
            )
            .optional()?;
        if latest.as_deref() == Some(hash.as_str()) {
            return Ok(None);
        }
        let version = NoteVersion {
            id: new_note_id(),
            note_id: new.note_id,
            created_at: now_rfc3339(),
            content_markdown: new.content_markdown,
            content_hash: hash,
            source: new.source.unwrap_or_else(|| "manual".to_string()),
            op: new.op,
            provider_id: new.provider_id,
        };
        self.conn.execute(
            "INSERT INTO note_versions \
             (id, note_id, created_at, content_md, content_hash, source, op, provider_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                version.id,
                version.note_id,
                version.created_at,
                version.content_markdown,
                version.content_hash,
                version.source,
                version.op,
                version.provider_id,
            ],
        )?;
        Ok(Some(version))
    }

    /// A note's stored content snapshots, most recent first.
    pub fn list_versions(&self, note_id: &str) -> Result<Vec<NoteVersion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, note_id, created_at, content_md, content_hash, source, op, provider_id \
             FROM note_versions WHERE note_id = ?1 ORDER BY created_at DESC, rowid DESC",
        )?;
        let versions = stmt
            .query_map(params![note_id], row_to_version)?
            .collect::<Result<Vec<_>>>()?;
        Ok(versions)
    }

    /// Restore a stored version's content into the (live) note as a new, undoable
    /// change — non-destructive: the prior content is preserved as history and a
    /// fresh `restore` version is recorded. Returns the updated note, or `None`
    /// if the version or note is gone (or the note is trashed).
    pub fn restore_version(&self, note_id: &str, version_id: &str) -> Result<Option<Note>> {
        let content: Option<String> = self
            .conn
            .query_row(
                "SELECT content_md FROM note_versions WHERE id = ?1 AND note_id = ?2",
                params![version_id, note_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(content) = content else {
            return Ok(None);
        };
        let updated = self.update_note(
            note_id,
            NoteUpdate {
                content_markdown: Some(content.clone()),
                ..Default::default()
            },
        )?;
        if updated.is_some() {
            self.insert_version(NewNoteVersion {
                note_id: note_id.to_string(),
                content_markdown: content,
                source: Some("op".to_string()),
                op: Some("restore".to_string()),
                provider_id: None,
            })?;
        }
        Ok(updated)
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
        title_auto: row.get("title_auto")?,
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

fn row_to_event(row: &Row) -> Result<NoteEvent> {
    Ok(NoteEvent {
        id: row.get("id")?,
        note_id: row.get("note_id")?,
        source_type: row.get("source_type")?,
        raw_text: row.get("raw_text")?,
        processed_text: row.get("processed_text")?,
        operation: row.get("operation")?,
        provider_id: row.get("provider_id")?,
        model_id: row.get("model_id")?,
        model_version: row.get("model_version")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_version(row: &Row) -> Result<NoteVersion> {
    Ok(NoteVersion {
        id: row.get("id")?,
        note_id: row.get("note_id")?,
        created_at: row.get("created_at")?,
        content_markdown: row.get("content_md")?,
        content_hash: row.get("content_hash")?,
        source: row.get("source")?,
        op: row.get("op")?,
        provider_id: row.get("provider_id")?,
    })
}

/// Stable 64-bit FNV-1a hash of `s`, hex-encoded — used to dedup identical note
/// snapshots (a no-op save hashes the same as the previous version). Stable
/// across runs so the dedup survives restarts.
fn content_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
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
    fn auto_title_follows_content_until_named() {
        let db = Database::open_in_memory().unwrap();
        let n = db.create_note(note("Erste Zeile\nmehr")).unwrap();
        assert!(n.title_auto);
        assert_eq!(n.title, "Erste Zeile");

        // Editing the content re-derives the title while it's still auto.
        let n = db
            .update_note(
                &n.id,
                NoteUpdate {
                    content_markdown: Some("# Neue Überschrift\nText".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(n.title_auto);
        assert_eq!(n.title, "Neue Überschrift");

        // Naming the note pins the title; later content edits leave it alone.
        let n = db
            .update_note(
                &n.id,
                NoteUpdate {
                    title: Some("Mein Titel".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(!n.title_auto);
        let n = db
            .update_note(
                &n.id,
                NoteUpdate {
                    content_markdown: Some("Komplett anderer Inhalt".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(!n.title_auto);
        assert_eq!(n.title, "Mein Titel");

        // Clearing the title hands control back to the auto-derivation.
        let n = db
            .update_note(
                &n.id,
                NoteUpdate {
                    title: Some("   ".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(n.title_auto);
        assert_eq!(n.title, "Komplett anderer Inhalt");
    }

    #[test]
    fn delete_is_soft_and_hides_from_list() {
        let db = Database::open_in_memory().unwrap();
        let created = db.create_note(note("bye")).unwrap();
        assert!(db.delete_note(&created.id).unwrap());
        assert!(db.list_notes(NoteScope::Active, NoteSort::Modified).unwrap().is_empty());
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
        let ids: Vec<String> = db
            .list_notes(NoteScope::Active, NoteSort::Modified)
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn search_matches_title_and_content_literally() {
        let db = Database::open_in_memory().unwrap();
        db.create_note(note("Rust und WebGPU")).unwrap();
        db.create_note(note("Einkaufsliste")).unwrap();
        assert_eq!(db.search_notes("webgpu", NoteScope::Active).unwrap().len(), 1);
        // Wildcards are treated literally, not as SQL LIKE patterns.
        assert_eq!(db.search_notes("%", NoteScope::Active).unwrap().len(), 0);
    }

    #[test]
    fn scopes_partition_active_archived_trash() {
        let db = Database::open_in_memory().unwrap();
        let active = db.create_note(note("active one")).unwrap();
        let arch = db.create_note(note("archived one")).unwrap();
        let trash = db.create_note(note("trashed one")).unwrap();
        assert!(db.set_archived(&arch.id, true).unwrap());
        assert!(db.delete_note(&trash.id).unwrap());

        let ids = |scope| {
            db.list_notes(scope, NoteSort::Modified)
                .unwrap()
                .into_iter()
                .map(|n| n.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(NoteScope::Active), vec![active.id.clone()]);
        assert_eq!(ids(NoteScope::Archived), vec![arch.id.clone()]);
        assert_eq!(ids(NoteScope::Trash), vec![trash.id.clone()]);

        // Search is scoped too: a term in the archived note isn't found in Active.
        assert!(db.search_notes("archived", NoteScope::Active).unwrap().is_empty());
        assert_eq!(db.search_notes("archived", NoteScope::Archived).unwrap().len(), 1);
    }

    #[test]
    fn sort_by_title_is_case_insensitive() {
        let db = Database::open_in_memory().unwrap();
        db.create_note(note("banana")).unwrap();
        db.create_note(note("Apple")).unwrap();
        let titles: Vec<String> = db
            .list_notes(NoteScope::Active, NoteSort::Title)
            .unwrap()
            .into_iter()
            .map(|n| n.title)
            .collect();
        assert_eq!(titles, vec!["Apple", "banana"]);
    }

    #[test]
    fn restore_brings_a_trashed_note_back_to_active() {
        let db = Database::open_in_memory().unwrap();
        let n = db.create_note(note("oops")).unwrap();
        db.delete_note(&n.id).unwrap();
        assert!(db.restore_note(&n.id).unwrap());
        assert_eq!(db.list_notes(NoteScope::Active, NoteSort::Modified).unwrap().len(), 1);
        // Restoring a live note is a no-op.
        assert!(!db.restore_note(&n.id).unwrap());
    }

    #[test]
    fn hard_delete_removes_the_row_and_dependents() {
        let db = Database::open_in_memory().unwrap();
        let n = db.create_note(note("gone")).unwrap();
        db.insert_version(NewNoteVersion {
            note_id: n.id.clone(),
            content_markdown: "gone".into(),
            ..Default::default()
        })
        .unwrap();
        assert!(db.hard_delete_note(&n.id).unwrap());
        assert!(db.get_note(&n.id).unwrap().is_none());
        assert!(db.list_versions(&n.id).unwrap().is_empty());
        assert!(!db.hard_delete_note(&n.id).unwrap());
    }

    #[test]
    fn purge_trash_removes_only_old_trashed_notes() {
        let db = Database::open_in_memory().unwrap();
        // A trashed note with an old deleted_at, and a live one.
        let old = db.create_note(note("old")).unwrap();
        let live = db.create_note(note("live")).unwrap();
        db.conn
            .execute(
                "UPDATE notes SET deleted_at = '2020-01-01T00:00:00.000Z' WHERE id = ?1",
                params![old.id],
            )
            .unwrap();
        let purged = db.purge_trash("2026-01-01T00:00:00.000Z").unwrap();
        assert_eq!(purged, 1);
        assert!(db.get_note(&old.id).unwrap().is_none());
        assert!(db.get_note(&live.id).unwrap().is_some());
    }

    #[test]
    fn versions_dedup_and_restore() {
        let db = Database::open_in_memory().unwrap();
        let n = db.create_note(note("v1")).unwrap();
        assert!(db
            .insert_version(NewNoteVersion {
                note_id: n.id.clone(),
                content_markdown: "v1".into(),
                ..Default::default()
            })
            .unwrap()
            .is_some());
        // Identical content is deduped (no-op save adds nothing).
        assert!(db
            .insert_version(NewNoteVersion {
                note_id: n.id.clone(),
                content_markdown: "v1".into(),
                ..Default::default()
            })
            .unwrap()
            .is_none());
        let v1 = db.list_versions(&n.id).unwrap()[0].clone();
        // A second, different snapshot is stored.
        db.update_note(
            &n.id,
            NoteUpdate {
                content_markdown: Some("v2".into()),
                ..Default::default()
            },
        )
        .unwrap();
        db.insert_version(NewNoteVersion {
            note_id: n.id.clone(),
            content_markdown: "v2".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(db.list_versions(&n.id).unwrap().len(), 2);
        // Restoring v1 writes its content back and records a fresh version.
        let restored = db.restore_version(&n.id, &v1.id).unwrap().unwrap();
        assert_eq!(restored.content_markdown, "v1");
        assert_eq!(db.list_versions(&n.id).unwrap().len(), 3);
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
    fn events_record_and_list_most_recent_first() {
        use exoquill_core::note::NewNoteEvent;
        let db = Database::open_in_memory().unwrap();
        let n = db.create_note(note("draft")).unwrap();
        db.insert_event(NewNoteEvent {
            note_id: n.id.clone(),
            source_type: "format".into(),
            raw_text: Some("draft".into()),
            processed_text: Some("Draft.".into()),
            operation: Some("quick_format".into()),
            provider_id: Some("formatter.mock".into()),
            ..Default::default()
        })
        .unwrap();
        db.insert_event(NewNoteEvent {
            note_id: n.id.clone(),
            source_type: "ocr".into(),
            processed_text: Some("scanned".into()),
            ..Default::default()
        })
        .unwrap();
        let events = db.list_events(&n.id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source_type, "ocr"); // most recent first
        assert_eq!(events[1].raw_text.as_deref(), Some("draft"));
        // A note with no events lists empty.
        let other = db.create_note(note("x")).unwrap();
        assert!(db.list_events(&other.id).unwrap().is_empty());
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
