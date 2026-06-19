//! Tauri job commands and the event sink bridging the core job queue to the
//! webview. The Format action runs as an async job through the queue.

use std::sync::Arc;

use exoquill_ai::formatter::FormatRequest;
use exoquill_ai::ocr::OcrRequest;
use exoquill_ai::stt::SttRequest;
use exoquill_ai::tts::{TtsRequest, TtsResponse};
use exoquill_core::note::NoteUpdate;
use exoquill_core::{CancelToken, Event, EventSink, Job};
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

/// Run OCR on an image and append the recognized text to the note, as an async
/// job. Returns the job id; the result is persisted and announced via an event.
#[tauri::command]
pub fn run_ocr(
    state: State<AppState>,
    note_id: String,
    image_bytes: Vec<u8>,
) -> Result<String, String> {
    let note = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_note(&note_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "note not found".to_string())?
    };
    let db = Arc::clone(&state.db);
    let ocr = Arc::clone(&state.ocr);
    let target_id = note_id.clone();

    let job_id = state.jobs.enqueue(
        "ocr",
        Some(note_id),
        Box::new(move |handle| {
            handle.report_progress(0.3);
            let request = OcrRequest {
                image_bytes,
                languages: "deu+eng".into(),
            };
            let response = ocr
                .run(request, handle.cancel_token())
                .map_err(|e| e.to_string())?;
            handle.report_progress(0.8);
            let mut content = note.content_markdown.clone();
            if !content.trim().is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(&response.text);
            db.lock()
                .map_err(|e| e.to_string())?
                .update_note(
                    &target_id,
                    NoteUpdate {
                        content_markdown: Some(content),
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        }),
    );
    Ok(job_id)
}

/// Format a short snippet (e.g. an editor selection) and return the result
/// directly. Synchronous: selections are short and the result must land back at
/// the exact cursor position. Whole-note formatting uses the job queue instead.
#[tauri::command]
pub fn format_text(
    state: State<AppState>,
    text: String,
    instruction: Option<String>,
) -> Result<String, String> {
    let operation = if instruction.is_some() {
        "custom_format"
    } else {
        "quick_format"
    };
    let request = FormatRequest {
        text,
        source: "manual".into(),
        language_mode: "de_en_terms".into(),
        operation: operation.into(),
        instruction,
        custom_terms: Vec::new(),
    };
    let response = state
        .formatter
        .run(request, &CancelToken::new())
        .map_err(|e| e.to_string())?;
    Ok(response.formatted_text)
}

/// Transcribe one dictation segment's PCM samples and return the recognized
/// text. Synchronous like `format_text`: segments are short and the transcript
/// must land back at the editor cursor immediately. The frontend captures and
/// segments the audio; only inference crosses into the Whisper sidecar.
#[tauri::command]
pub fn transcribe(
    state: State<AppState>,
    samples: Vec<f32>,
    sample_rate: u32,
    language_mode: Option<String>,
    custom_terms: Option<Vec<String>>,
) -> Result<String, String> {
    let request = SttRequest {
        samples,
        sample_rate,
        language_mode: language_mode.unwrap_or_else(|| "de_en_terms".into()),
        custom_terms: custom_terms.unwrap_or_default(),
    };
    let response = state
        .stt
        .run(request, &CancelToken::new())
        .map_err(|e| e.to_string())?;
    Ok(response.text)
}

/// Synthesize speech for `text` with the local TTS provider, returning the PCM
/// samples for the frontend to play. Errors when no local TTS is available so
/// the UI can fall back to system speech.
#[tauri::command]
pub fn tts_speak(state: State<AppState>, text: String) -> Result<TtsResponse, String> {
    let tts = state
        .tts
        .as_ref()
        .ok_or_else(|| "no local TTS provider".to_string())?;
    let request = TtsRequest {
        text,
        voice_id: "de".into(),
        speed: 1.0,
    };
    tts.run(request, &CancelToken::new())
        .map_err(|e| e.to_string())
}
