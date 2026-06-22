//! Tauri commands for the notes core. Thin wrappers that lock the database and
//! map persistence errors to strings for the IPC bridge.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_ai::ocr::OcrProvider;
use exoquill_ai::stt::SpeechToTextProvider;
use exoquill_ai::tts::TextToSpeechProvider;
use exoquill_core::note::{
    NewNote, NewNoteVersion, Note, NoteEvent, NoteScope, NoteSort, NoteSource, NoteUpdate,
    NoteVersion,
};
use exoquill_core::{CancelToken, JobQueue};
use exoquill_db::Database;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// Application state shared across commands.
pub struct AppState {
    /// Shared so job tasks can persist results from the worker thread.
    pub db: Arc<Mutex<Database>>,
    pub jobs: JobQueue,
    /// Per-call llama.cpp (`llama-cli`) when reachable, otherwise the mock. Used
    /// as the formatter fallback when the persistent server can't start.
    pub formatter: Arc<dyn FormatterProvider>,
    /// `(llama-server.exe, model)` paths for the persistent formatter server, or
    /// `None` when the runtime/model isn't available. Resolved once at setup.
    pub llama_server_paths: Option<(PathBuf, PathBuf)>,
    /// The persistent llama-server, started lazily on first format and kept alive
    /// (model resident) so chunked formatting is fast. Dropping it kills it.
    pub llama_server: Mutex<Option<exoquill_ai::LlamaServer>>,
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
    /// This is the Piper (or external-URL XTTS) provider resolved at setup; it's
    /// the fallback when the auto-spawned XTTS sidecar isn't running.
    pub tts: Option<Arc<dyn TextToSpeechProvider>>,
    /// `(python, xtts-server.py)` paths to auto-spawn the XTTS sidecar, or `None`
    /// when not configured (then TTS uses `tts` above). Experimental / dev.
    pub xtts_paths: Option<(PathBuf, PathBuf)>,
    /// The XTTS sidecar, warmed up on demand (when the UI selects the XTTS
    /// backend) and kept alive. Dropping it kills the Python process. Not started
    /// at launch — that's what froze the UI when two sidecars loaded at once.
    pub xtts_server: Mutex<Option<exoquill_ai::XttsServer>>,
    /// Guards against starting two XTTS sidecars when `warm_tts` is called twice
    /// before the first finishes loading.
    pub xtts_warming: std::sync::atomic::AtomicBool,
    /// `(python, zonos-server.py, voices_dir)` to spawn the Zonos sidecar, or
    /// `None` when not configured. `voices_dir` holds the reference `.wav` clips
    /// (one per voice). Apache-2.0 weights, but needs a CUDA GPU.
    pub zonos_paths: Option<(PathBuf, PathBuf, PathBuf)>,
    /// The Zonos sidecar, warmed up on demand (when the UI selects Zonos) and
    /// kept alive. Dropping it kills the Python process.
    pub zonos_server: Mutex<Option<exoquill_ai::ZonosServer>>,
    /// Guards against starting two Zonos sidecars concurrently (see above).
    pub zonos_warming: std::sync::atomic::AtomicBool,
    /// Cancellation for the in-progress read-aloud speech-prep pass. `begin_read`
    /// installs a fresh token, `cancel_read` trips it, and each `prepare_speech`
    /// chunk runs under it so a cancel stops the streaming llama generation
    /// mid-flight instead of letting the chunk run to completion.
    pub read_cancel: Mutex<CancelToken>,
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

#[tauri::command(async)]
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

#[tauri::command(async)]
pub fn get_note(state: State<AppState>, id: String) -> CommandResult<Option<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_note(&id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn update_note(
    state: State<AppState>,
    id: String,
    update: NoteUpdate,
) -> CommandResult<Option<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_note(&id, update).map_err(|e| e.to_string())
}

/// Move a note to the trash (soft-delete). Returns `true` if a live note moved.
#[tauri::command(async)]
pub fn delete_note(state: State<AppState>, id: String) -> CommandResult<bool> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_note(&id).map_err(|e| e.to_string())
}

/// Restore a trashed note back to Active.
#[tauri::command(async)]
pub fn restore_note(state: State<AppState>, id: String) -> CommandResult<bool> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.restore_note(&id).map_err(|e| e.to_string())
}

/// Archive or un-archive a live note.
#[tauri::command(async)]
pub fn set_archived(state: State<AppState>, id: String, archived: bool) -> CommandResult<bool> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_archived(&id, archived).map_err(|e| e.to_string())
}

/// Permanently delete a note (and its events + versions). No undo.
#[tauri::command(async)]
pub fn hard_delete_note(state: State<AppState>, id: String) -> CommandResult<bool> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.hard_delete_note(&id).map_err(|e| e.to_string())
}

/// Permanently delete trashed notes older than `before` (an RFC-3339 timestamp,
/// e.g. now − 30 days; the frontend computes the cutoff). Returns the count.
#[tauri::command(async)]
pub fn purge_trash(state: State<AppState>, before: String) -> CommandResult<usize> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.purge_trash(&before).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn list_notes(
    state: State<AppState>,
    scope: Option<NoteScope>,
    sort: Option<NoteSort>,
) -> CommandResult<Vec<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_notes(scope.unwrap_or_default(), sort.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn search_notes(
    state: State<AppState>,
    query: String,
    scope: Option<NoteScope>,
) -> CommandResult<Vec<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.search_notes(&query, scope.unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Record a content snapshot for the edit history (deduped by content hash, so
/// no-op saves add nothing). Returns the stored version, or `None` if deduped.
#[tauri::command(async)]
pub fn snapshot_note_version(
    state: State<AppState>,
    version: NewNoteVersion,
) -> CommandResult<Option<NoteVersion>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.insert_version(version).map_err(|e| e.to_string())
}

/// A note's edit-history versions (diff timeline), most recent first.
#[tauri::command(async)]
pub fn list_note_history(
    state: State<AppState>,
    note_id: String,
) -> CommandResult<Vec<NoteVersion>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_versions(&note_id).map_err(|e| e.to_string())
}

/// Restore a stored version's content into the note as a new, undoable change
/// (non-destructive). Returns the updated note, or `None` if it's gone.
#[tauri::command(async)]
pub fn restore_note_version(
    state: State<AppState>,
    note_id: String,
    version_id: String,
) -> CommandResult<Option<Note>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.restore_version(&note_id, &version_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn resolve_target_note(state: State<AppState>, active: Option<String>) -> CommandResult<Note> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.resolve_target_note(active.as_deref())
        .map_err(|e| e.to_string())
}

/// The recorded events for a note (formatting/OCR history + undo safety net),
/// most recent first.
#[tauri::command(async)]
pub fn list_note_events(state: State<AppState>, note_id: String) -> CommandResult<Vec<NoteEvent>> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_events(&note_id).map_err(|e| e.to_string())
}

/// Export a note's Markdown to a file the user picks (native save dialog).
/// Returns the saved path, or `None` if the user cancelled. Notes are stored as
/// Markdown, so this writes `content_markdown` verbatim.
#[tauri::command(async)]
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
