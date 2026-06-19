//! Tauri job commands and the event sink bridging the core job queue to the
//! webview. The Format action runs as an async job through the queue.

use std::sync::Arc;

use exoquill_ai::formatter::FormatRequest;
use exoquill_core::note::NoteUpdate;
use exoquill_core::{Event, EventSink, Job};
use tauri::{AppHandle, Emitter, State};

use crate::notes::AppState;

/// Delivers core events to the frontend over Tauri's event channel.
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: Event) {
        // The frontend listens on "backend-event" and switches on `type`.
        let _ = self.app.emit("backend-event", event);
    }
}

/// Quick-format the whole note via the formatter provider, as an async job.
/// Returns the job id immediately; the result is persisted and announced via
/// a `job_updated` event.
#[tauri::command]
pub fn format_note(state: State<AppState>, note_id: String) -> Result<String, String> {
    let note = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_note(&note_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "note not found".to_string())?
    };

    let db = Arc::clone(&state.db);
    let formatter = Arc::clone(&state.formatter);
    let target_id = note_id.clone();

    let job_id = state.jobs.enqueue(
        "format",
        Some(note_id),
        Box::new(move |handle| {
            handle.report_progress(0.2);
            let request = FormatRequest {
                text: note.content_markdown.clone(),
                source: "manual".into(),
                language_mode: note.language_mode.clone(),
                operation: "quick_format".into(),
                instruction: None,
                custom_terms: Vec::new(),
            };
            let response = formatter
                .run(request, handle.cancel_token())
                .map_err(|e| e.to_string())?;
            handle.report_progress(0.8);
            db.lock()
                .map_err(|e| e.to_string())?
                .update_note(
                    &target_id,
                    NoteUpdate {
                        content_markdown: Some(response.formatted_text),
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        }),
    );

    Ok(job_id)
}

#[tauri::command]
pub fn cancel_job(state: State<AppState>, id: String) {
    state.jobs.cancel(&id);
}

#[tauri::command]
pub fn list_jobs(state: State<AppState>) -> Vec<Job> {
    state.jobs.jobs()
}
