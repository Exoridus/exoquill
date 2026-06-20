//! Tauri commands for the notes core. Thin wrappers that lock the database and
//! map persistence errors to strings for the IPC bridge.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_ai::ocr::OcrProvider;
use exoquill_ai::stt::SpeechToTextProvider;
use exoquill_ai::tts::TextToSpeechProvider;
use exoquill_core::note::{NewNote, Note, NoteEvent, NoteSource, NoteUpdate};
use exoquill_core::JobQueue;
use exoquill_db::Database;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// Application state shared across commands.
pub struct AppState {
    /// Shared so job tasks can persist results from the worker thread.
    pub db: Arc<Mutex<Database>>,
    pub jobs: JobQueue,
    pub formatter: Arc<dyn FormatterProvider>,
    pub ocr: Arc<dyn OcrProvider>,
    /// Per-call Whisper (`whisper-cli`) when reachable, otherwise the mock. Used
    /// as the dictation fallback when the persistent server can't start.
    pub stt: Arc<dyn SpeechToTextProvider>,
    /// `(whisper-server.exe, model)` paths for the persistent low-latency server,
    /// or `None` when the runtime/model isn't available. Resolved once at setup.
    pub whisper_server_paths: Option<(PathBuf, PathBuf)>,
    /// The persistent whisper-server, started lazily on first dictation and kept
    /// alive (model resident on the GPU) so partial transcripts are cheap.
    /// Dropping it kills the server.
    pub whisper_server: Mutex<Option<exoquill_ai::WhisperServer>>,
    /// `None` when no local TTS is available; the UI falls back to system speech.
    pub tts: Option<Arc<dyn TextToSpeechProvider>>,
    /// The active dictation session, if capturing. Guarded so start/stop and the
    /// worker never race on it.
    pub dictation: Mutex<Option<crate::dictation::DictationController>>,
    /// The frozen screenshot for an in-progress region-OCR selection (set when
    /// the overlay opens, drained when the user selects a region or cancels).
    pub region_capture: Mutex<Option<exoquill_capture::ScreenShot>>,
    /// Path to the Silero VAD ONNX model, when built with the `silero` feature
    /// and the model is present. `None` falls dictation back to the energy gate.
    #[cfg(feature = "silero")]
    pub silero_model_path: Option<PathBuf>,
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

/// The recorded events for a note (formatting/OCR history + undo safety net),
/// most recent first.
#[tauri::command]
pub fn list_note_events(state: State<AppState>, note_id: String) -> CommandResult<Vec<NoteEvent>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_events(&note_id).map_err(|e| e.to_string())
}

/// Export a note's Markdown to a file the user picks (native save dialog).
/// Returns the saved path, or `None` if the user cancelled. Notes are stored as
/// Markdown, so this writes `content_markdown` verbatim.
#[tauri::command]
pub fn export_note(
    state: State<AppState>,
    app: AppHandle,
    id: String,
) -> CommandResult<Option<String>> {
    let note = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_note(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "note not found".to_string())?
    };
    let suggested = format!("{}.md", sanitize_filename(&note.title));
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_file_name(&suggested)
        .blocking_save_file()
    else {
        return Ok(None); // user cancelled
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, note.content_markdown.as_bytes()).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Make a note title safe to use as a file name (replace path-hostile chars).
fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "note".to_string()
    } else {
        trimmed.to_string()
    }
}
