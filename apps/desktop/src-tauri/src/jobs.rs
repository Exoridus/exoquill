//! Tauri job commands and the event sink bridging the core job queue to the
//! webview. The Format action runs as an async job through the queue.

use std::sync::Arc;

use base64::Engine;
use exoquill_ai::formatter::FormatRequest;
use exoquill_ai::ocr::{OcrLayout, OcrRequest};
use exoquill_ai::tts::{TtsRequest, TtsResponse};
use exoquill_core::note::{NewNoteEvent, NoteUpdate};
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
            let original = note.content_markdown.clone();
            let request = FormatRequest {
                text: original.clone(),
                source: "manual".into(),
                language_mode: note.language_mode.clone(),
                operation: "quick_format".into(),
                instruction: None,
                custom_terms: Vec::new(),
            };
            let provider_id = formatter.id().to_string();
            let response = formatter
                .run(request, handle.cancel_token())
                .map_err(|e| e.to_string())?;
            handle.report_progress(0.8);
            let formatted = response.formatted_text;
            let db = db.lock().map_err(|e| e.to_string())?;
            db.update_note(
                &target_id,
                NoteUpdate {
                    content_markdown: Some(formatted.clone()),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
            // Record the operation (keeps the pre-format text as the undo safety
            // net, D6). Best-effort — a history write must not fail the format.
            let _ = db.insert_event(NewNoteEvent {
                note_id: target_id.clone(),
                source_type: "format".into(),
                raw_text: Some(original),
                processed_text: Some(formatted),
                operation: Some("quick_format".into()),
                provider_id: Some(provider_id),
                ..Default::default()
            });
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
            let provider_id = ocr.id().to_string();
            let response = ocr
                .run(request, handle.cancel_token())
                .map_err(|e| e.to_string())?;
            handle.report_progress(0.8);
            let recognized = response.text;
            let mut content = note.content_markdown.clone();
            if !content.trim().is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(&recognized);
            let db = db.lock().map_err(|e| e.to_string())?;
            db.update_note(
                &target_id,
                NoteUpdate {
                    content_markdown: Some(content),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
            let _ = db.insert_event(NewNoteEvent {
                note_id: target_id.clone(),
                source_type: "ocr".into(),
                processed_text: Some(recognized),
                operation: Some("ocr".into()),
                provider_id: Some(provider_id),
                ..Default::default()
            });
            Ok(())
        }),
    );
    Ok(job_id)
}

/// Recognize an image with word bounding boxes + layout-preserving text, for
/// the selectable OCR overlay. Synchronous and does not touch any note — the UI
/// decides what to insert. Returns boxes only with the real Tesseract provider;
/// the mock returns text alone.
#[tauri::command]
pub fn ocr_image(state: State<AppState>, image_bytes: Vec<u8>) -> Result<OcrLayout, String> {
    let request = OcrRequest {
        image_bytes,
        languages: "deu+eng".into(),
    };
    state
        .ocr
        .run_layout(request, &CancelToken::new())
        .map_err(|e| e.to_string())
}

/// The full frozen screenshot for the region-OCR overlay, as a PNG data URL.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionCapture {
    pub data_url: String,
}

/// A selected region: the cropped image (PNG data URL) plus its OCR layout.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionOcr {
    pub data_url: String,
    pub layout: OcrLayout,
}

fn png_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Hand the frozen screenshot to the selection overlay so it can display the
/// monitor it covers.
#[tauri::command]
pub fn get_region_capture(state: State<AppState>) -> Result<RegionCapture, String> {
    let guard = state.region_capture.lock().map_err(|e| e.to_string())?;
    let shot = guard.as_ref().ok_or("no region capture in progress")?;
    Ok(RegionCapture {
        data_url: png_data_url(&shot.to_png()?),
    })
}

/// OCR the selected region. The rectangle is in the overlay's logical coordinates
/// (CSS px relative to the monitor); it's mapped to pixels and cropped from the
/// frozen screenshot, which is then consumed (freed). Returns the cropped image
/// + layout for the result overlay.
#[tauri::command]
pub fn ocr_region(
    state: State<AppState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<RegionOcr, String> {
    let png = {
        let mut guard = state.region_capture.lock().map_err(|e| e.to_string())?;
        let shot = guard.as_ref().ok_or("no region capture in progress")?;
        let cropped = shot.crop_png(x, y, width, height)?;
        *guard = None; // free the full-screen capture now that we've cropped it
        cropped
    };
    let png = png.ok_or("empty selection")?;
    let layout = state
        .ocr
        .run_layout(
            OcrRequest {
                image_bytes: png.clone(),
                languages: "deu+eng".into(),
            },
            &CancelToken::new(),
        )
        .map_err(|e| e.to_string())?;
    Ok(RegionOcr {
        data_url: png_data_url(&png),
        layout,
    })
}

/// Discard an in-progress region capture (the overlay was cancelled).
#[tauri::command]
pub fn cancel_region_ocr(state: State<AppState>) -> Result<(), String> {
    *state.region_capture.lock().map_err(|e| e.to_string())? = None;
    Ok(())
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
