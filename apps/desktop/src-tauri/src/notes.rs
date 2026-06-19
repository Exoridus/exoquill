//! Tauri commands for the notes core. Thin wrappers that lock the database and
//! map persistence errors to strings for the IPC bridge.

use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_core::note::{NewNote, Note, NoteSource, NoteUpdate};
use exoquill_core::JobQueue;
use exoquill_db::Database;
use tauri::State;

/// Application state shared across commands.
pub struct AppState {
    /// Shared so job tasks can persist results from the worker thread.
    pub db: Arc<Mutex<Database>>,
    pub jobs: JobQueue,
    pub formatter: Arc<dyn FormatterProvider>,
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn create_note(
    state: State<AppState>,
    content_markdown: String,
    source: Option<NoteSource>,
    language_mode: Option<String>,
) -> CommandResult<Note> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.create_note(NewNote {
        title: None,
        content_markdown,
        source: source.unwrap_or_default(),
        language_mode,
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note(state: State<AppState>, id: String) -> CommandResult<Option<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_note(
    state: State<AppState>,
    id: String,
    update: NoteUpdate,
) -> CommandResult<Option<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_note(&id, update).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(state: State<AppState>, id: String) -> CommandResult<bool> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notes(state: State<AppState>) -> CommandResult<Vec<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_notes(state: State<AppState>, query: String) -> CommandResult<Vec<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.search_notes(&query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resolve_target_note(state: State<AppState>, active: Option<String>) -> CommandResult<Note> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.resolve_target_note(active.as_deref())
        .map_err(|e| e.to_string())
}
