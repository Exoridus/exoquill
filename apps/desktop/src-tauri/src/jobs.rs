//! Tauri job commands and the event sink bridging the core job queue to the
//! webview. The Format action runs as an async job through the queue.

use std::sync::Arc;

use base64::Engine;
use exoquill_ai::formatter::{FormatRequest, FormatterProvider};
use exoquill_ai::ocr::{OcrLayout, OcrRequest};
use exoquill_ai::provider::{Health, Provider};
use exoquill_ai::tts::{TextToSpeechProvider, TtsRequest, TtsVoice};
use exoquill_core::note::{NewNoteEvent, NoteUpdate};
use exoquill_core::{CancelToken, Event, EventSink, Job};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::notes::AppState;

/// Clone the provider currently in a mutex-guarded slot, if any (poisoned → None).
fn slot_provider(
    slot: &std::sync::Mutex<Option<Arc<dyn TextToSpeechProvider>>>,
) -> Option<Arc<dyn TextToSpeechProvider>> {
    slot.lock().ok().and_then(|g| g.clone())
}

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

/// Run a blocking command body on the dedicated blocking thread pool. This keeps
/// it off the UI thread (so the webview never freezes) AND off the Tokio worker
/// pool — provider calls use `reqwest::blocking`, whose internal runtime panics
/// if dropped inside an async/Tokio context. The closure gets `&AppState`,
/// resolved from the handle on the blocking thread.
pub(crate) async fn off_thread<T, F>(app: AppHandle, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(app.state::<AppState>().inner()))
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

/// The formatter for a request: the persistent llama-server (started on first
/// use, model resident → fast for chunked formatting), else the per-call
/// fallback in [`AppState`]. A server-start failure falls through silently.
fn ensure_formatter(state: &AppState) -> Arc<dyn FormatterProvider> {
    if let Some((binary, model)) = state.llama_server_paths.clone() {
        let mut slot = match state.llama_server.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.is_none() {
            if let Ok(server) = exoquill_ai::LlamaServer::start(&binary, &model) {
                *slot = Some(server);
            }
        }
        if let Some(server) = slot.as_ref() {
            if let Ok(client) = server.client() {
                return Arc::new(client) as Arc<dyn FormatterProvider>;
            }
        }
    }
    Arc::clone(&state.formatter)
}

/// The TTS provider for a request, honoring the backend the UI picked.
/// `provider` is the voice's backend (`"piper"` | `"xtts"` | `"zonos"`); `None`
/// keeps the legacy auto behavior (prefer a warm XTTS sidecar, else Piper).
///
/// - `"piper"` → the bundled Piper provider, always available.
/// - `"zonos"` → the auto-spawned Zonos sidecar (Apache-2.0, GPU) once warmed up.
/// - `"xtts"` / auto → the auto-spawned XTTS sidecar once it has warmed up
///   (multilingual, better with technical terms).
///
/// A sidecar that's still warming up falls back to Piper so playback works rather
/// than failing. Returns `None` only when there's no local TTS at all (UI →
/// system speech).
fn tts_for(state: &AppState, provider: Option<&str>) -> Option<Arc<dyn TextToSpeechProvider>> {
    // An EXPLICITLY chosen sidecar backend never falls back to Piper: that would
    // play (and cache) the wrong — Piper — voice while the sidecar is still
    // warming. Return `None` instead so the UI knows it isn't ready yet.
    match provider {
        Some("piper") => slot_provider(&state.tts),
        Some("zonos") => state
            .zonos_server
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
            .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>),
        Some("chatterbox") => state
            .chatterbox_server
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
            .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>),
        Some("qwen3") => state
            .qwen3_server
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
            .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>),
        // Kokoro is a native provider (no sidecar): route straight to it.
        #[cfg(feature = "kokoro")]
        Some("kokoro") => slot_provider(&state.kokoro),
        Some("xtts") => {
            state.xtts_paths.lock().ok().and_then(|g| g.clone())?;
            state
                .xtts_server
                .lock()
                .ok()
                .and_then(|slot| slot.as_ref().and_then(|server| server.client()))
                .map(|client| Arc::new(client) as Arc<dyn TextToSpeechProvider>)
        }
        // Auto (no explicit backend): prefer a warm XTTS sidecar, else Piper.
        _ => {
            if state
                .xtts_paths
                .lock()
                .map(|g| g.is_some())
                .unwrap_or(false)
            {
                if let Ok(slot) = state.xtts_server.lock() {
                    if let Some(client) = slot.as_ref().and_then(|server| server.client()) {
                        return Some(Arc::new(client) as Arc<dyn TextToSpeechProvider>);
                    }
                }
            }
            slot_provider(&state.tts)
        }
    }
}

/// Quick-format the whole note via the formatter provider, as an async job.
/// Returns the job id immediately; the result is persisted and announced via
/// a `job_updated` event.
#[tauri::command(async)]
pub fn format_note(state: State<AppState>, note_id: String) -> Result<String, String> {
    let note = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_note(&note_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "note not found".to_string())?
    };

    let db = Arc::clone(&state.db);
    let formatter = ensure_formatter(&state);
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

#[tauri::command(async)]
pub fn cancel_job(state: State<AppState>, id: String) {
    state.jobs.cancel(&id);
}

#[tauri::command(async)]
pub fn list_jobs(state: State<AppState>) -> Vec<Job> {
    state.jobs.jobs()
}

/// Run OCR on an image and append the recognized text to the note, as an async
/// job. Returns the job id; the result is persisted and announced via an event.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn cancel_region_ocr(state: State<AppState>) -> Result<(), String> {
    *state.region_capture.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

/// Format a short snippet (e.g. an editor selection) and return the result
/// directly. Synchronous: selections are short and the result must land back at
/// the exact cursor position. Whole-note formatting uses the job queue instead.
#[tauri::command]
pub async fn format_text(
    app: AppHandle,
    text: String,
    instruction: Option<String>,
) -> Result<String, String> {
    off_thread(app, move |state| {
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
        let response = ensure_formatter(state)
            .run(request, &CancelToken::new())
            .map_err(|e| e.to_string())?;
        Ok(response.formatted_text)
    })
    .await
}

/// Instruction for the read-aloud "prepare for speech" pass. Lives here so the
/// whole speech-prep call is one IPC round-trip per chunk (text in, prose out).
/// Rewrites a screen-oriented note (tables, lists, code, links) into a linear,
/// spoken commentary — what a person would actually read aloud.
const SPEECH_INSTRUCTION: &str = "Schreibe den folgenden Text in eine Vorlese-Fassung um: \
einen zusammenhängenden, gut hörbaren Fließtext, wie ihn ein Mensch flüssig vorlesen würde, \
nicht wie eine technische Notiz. Wandle Tabellen in gesprochene Sätze um — lies jede Zeile als \
ganzen Satz, niemals als Spalten, Striche oder senkrechte Striche. Löse Aufzählungen und Listen \
in Fließtext auf, etwa „die wichtigsten Punkte sind A, B und C\". Entferne Codeblöcke, \
Dateipfade, URLs und Markdown-Zeichen und nenne ihren Inhalt nur knapp in Worten, wenn er \
wichtig ist. Schreibe Abkürzungen beim ersten Vorkommen aus und erkläre sie kurz, etwa „OCR, \
also Texterkennung\". Gib schwierigen Fachbegriffen einen kurzen erklärenden Halbsatz. Verwende \
kurze bis mittlere Sätze in kleinen Absätzen mit natürlichen Übergängen. Schreibe linear, ohne \
Verweise wie „siehe oben\" oder „in der Tabelle\" — beim Hören gibt es kein Oben oder Unten. \
Erfinde keine neuen Inhalte und lass nichts Wesentliches weg. Gib nur den reinen gesprochenen \
Fließtext zurück, ohne Überschriften und ohne Markdown.";

/// Kick off a sidecar backend's warm-up in a background thread. Idempotent: a
/// no-op when the backend is already warm, already warming, or not configured.
/// Shared by [`warm_tts`] (fire-and-forget) and [`ensure_tts_ready`] (which then
/// waits for it to finish).
fn warm_backend(state: &AppState, app: &AppHandle, provider: &str) {
    use std::sync::atomic::Ordering;
    match provider {
        "xtts" => {
            let Some((python, script)) = state.xtts_paths.lock().ok().and_then(|g| g.clone())
            else {
                return;
            };
            if state
                .xtts_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return; // already warm
            }
            if state.xtts_warming.swap(true, Ordering::SeqCst) {
                return; // already starting
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::XttsServer::start(python, script).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) = (server, state.xtts_server.lock()) {
                        *slot = Some(server);
                    }
                    state.xtts_warming.store(false, Ordering::SeqCst);
                }
            });
        }
        "zonos" => {
            let Some((python, script, voices)) =
                state.zonos_paths.lock().ok().and_then(|g| g.clone())
            else {
                return;
            };
            if state
                .zonos_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return;
            }
            if state.zonos_warming.swap(true, Ordering::SeqCst) {
                return;
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::ZonosServer::start(python, script, voices).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) = (server, state.zonos_server.lock()) {
                        *slot = Some(server);
                    }
                    state.zonos_warming.store(false, Ordering::SeqCst);
                }
            });
        }
        "chatterbox" => {
            let Some((python, script, voices)) =
                state.chatterbox_paths.lock().ok().and_then(|g| g.clone())
            else {
                return;
            };
            if state
                .chatterbox_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return;
            }
            if state.chatterbox_warming.swap(true, Ordering::SeqCst) {
                return;
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::ChatterboxServer::start(python, script, voices).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) = (server, state.chatterbox_server.lock()) {
                        *slot = Some(server);
                    }
                    state.chatterbox_warming.store(false, Ordering::SeqCst);
                }
            });
        }
        "qwen3" => {
            let Some((python, script, voices)) =
                state.qwen3_paths.lock().ok().and_then(|g| g.clone())
            else {
                return;
            };
            if state
                .qwen3_server
                .lock()
                .map(|s| s.is_some())
                .unwrap_or(false)
            {
                return;
            }
            if state.qwen3_warming.swap(true, Ordering::SeqCst) {
                return;
            }
            let handle = app.clone();
            std::thread::spawn(move || {
                let server = exoquill_ai::Qwen3Server::start(python, script, voices).ok();
                if let Some(state) = handle.try_state::<AppState>() {
                    if let (Some(server), Ok(mut slot)) = (server, state.qwen3_server.lock()) {
                        *slot = Some(server);
                    }
                    state.qwen3_warming.store(false, Ordering::SeqCst);
                }
            });
        }
        // Kokoro is native (no sidecar) — nothing to warm up; it's ready as soon
        // as the provider is built at setup. Handled in the `_` arm.
        _ => {}
    }
}

/// Warm up a TTS backend's sidecar in the background (idempotent). The UI calls
/// this when a backend is selected, so only the *active* backend ever loads —
/// never both at launch, which is what froze the UI. Piper needs no warm-up.
/// Returns immediately; synthesis falls back to Piper until the sidecar is ready.
#[tauri::command(async)]
pub fn warm_tts(state: State<AppState>, app: AppHandle, provider: String) {
    warm_backend(&state, &app, &provider);
}

/// Block until `provider`'s sidecar is warm (model loaded), starting its warm-up
/// if needed. Piper (and any unknown backend, which falls back to Piper) is
/// always ready at once. Returns `Ok` when synthesis can run, or `Err` on an
/// unconfigured backend, a warm-up failure, or timeout. The read-aloud UI calls
/// this when it finds the chosen voice cold, then retries the read automatically
/// once ready — so the user never has to click play a second time.
#[tauri::command(async)]
pub fn ensure_tts_ready(
    state: State<AppState>,
    app: AppHandle,
    provider: String,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let st = state.inner();
    let warm = || match provider.as_str() {
        "xtts" => st.xtts_server.lock().map(|s| s.is_some()).unwrap_or(false),
        "zonos" => st.zonos_server.lock().map(|s| s.is_some()).unwrap_or(false),
        "chatterbox" => st
            .chatterbox_server
            .lock()
            .map(|s| s.is_some())
            .unwrap_or(false),
        "qwen3" => st.qwen3_server.lock().map(|s| s.is_some()).unwrap_or(false),
        // Piper / Kokoro (native, no warm-up) / unknown → ready at once.
        _ => true,
    };
    let warming = || match provider.as_str() {
        "xtts" => st.xtts_warming.load(Ordering::SeqCst),
        "zonos" => st.zonos_warming.load(Ordering::SeqCst),
        "chatterbox" => st.chatterbox_warming.load(Ordering::SeqCst),
        "qwen3" => st.qwen3_warming.load(Ordering::SeqCst),
        _ => false,
    };
    let configured = match provider.as_str() {
        "xtts" => st.xtts_paths.lock().map(|g| g.is_some()).unwrap_or(false),
        "zonos" => st.zonos_paths.lock().map(|g| g.is_some()).unwrap_or(false),
        "chatterbox" => st
            .chatterbox_paths
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false),
        "qwen3" => st.qwen3_paths.lock().map(|g| g.is_some()).unwrap_or(false),
        _ => true,
    };

    if warm() {
        return Ok(());
    }
    if !configured {
        return Err(format!("{provider}-Backend ist nicht eingerichtet"));
    }

    warm_backend(st, &app, &provider);

    // Poll the shared state until the slot fills (success) or the warm-up thread
    // finishes without filling it (failure), bounded by a generous timeout — the
    // first ever run may download model weights.
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        std::thread::sleep(Duration::from_millis(300));
        if warm() {
            return Ok(());
        }
        if !warming() {
            // Warm-up finished but the slot is empty → it failed. Re-check once to
            // dodge the race where the slot is set just after the flag clears.
            if warm() {
                return Ok(());
            }
            return Err(format!("{provider}-Sidecar konnte nicht geladen werden"));
        }
        if Instant::now() >= deadline {
            return Err(format!("{provider}-Sidecar wurde nicht rechtzeitig bereit"));
        }
    }
}

/// Begin a read-aloud session: install a fresh cancel token. A subsequent
/// [`cancel_read`] trips it, which stops the streaming speech-prep generation
/// mid-flight (rather than letting the running chunk finish in the background).
#[tauri::command(async)]
pub fn begin_read(state: State<AppState>) {
    if let Ok(mut slot) = state.read_cancel.lock() {
        *slot = CancelToken::new();
    }
}

/// Cancel the in-progress read-aloud speech-prep (trips the session token). The
/// running `prepare_speech` chunk observes it between streamed tokens and bails.
#[tauri::command(async)]
pub fn cancel_read(state: State<AppState>) {
    if let Ok(slot) = state.read_cancel.lock() {
        slot.cancel();
    }
}

/// Rewrite one chunk of a note into clean, speakable prose for read-aloud, under
/// the current read session's cancel token. Synchronous (runs off the main
/// thread) and streamed inside the provider, so a cancel takes effect promptly.
#[tauri::command]
pub async fn prepare_speech(app: AppHandle, text: String) -> Result<String, String> {
    off_thread(app, move |state| {
        let cancel = state
            .read_cancel
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default();
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let request = FormatRequest {
            text,
            source: "manual".into(),
            language_mode: "de_en_terms".into(),
            operation: "speech_prep".into(),
            instruction: Some(SPEECH_INSTRUCTION.to_string()),
            custom_terms: Vec::new(),
        };
        let response = ensure_formatter(state)
            .run(request, &cancel)
            .map_err(|e| e.to_string())?;
        Ok(response.formatted_text)
    })
    .await
}

/// Synthesized audio for the webview: 16-bit little-endian mono PCM, base64'd.
/// Far cheaper over IPC than a `Vec<f32>` JSON array (~3× smaller and no
/// number-array parse), which matters because read-aloud calls this per sentence.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsAudio {
    pub pcm: String,
    pub sample_rate: u32,
}

/// Wrap raw 16-bit little-endian mono PCM in a RIFF/WAVE container at
/// `sample_rate`. Mirrors the frontend's old `encodeWav`, now that the file is
/// written natively (the webview can't trigger a real download in WebView2).
fn wav_from_pcm(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let data_len = pcm.len() as u32;
    let mut buf = Vec::with_capacity(44 + pcm.len());
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // format: PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // channels: mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate (mono, 2 B/sample)
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    buf.extend_from_slice(pcm);
    buf
}

/// Concatenate the read-aloud segments (base64 16-bit LE mono PCM) into one WAV
/// and write it to a file the user picks (native save dialog). Returns the saved
/// path, or `None` if the user cancelled. The webview can't trigger a real
/// download in WebView2, so — like [`crate::notes::export_note`] — the file is
/// written natively here.
#[tauri::command(async)]
pub fn export_audio(
    app: AppHandle,
    segments: Vec<String>,
    sample_rate: u32,
    suggested_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let engine = base64::engine::general_purpose::STANDARD;
    let mut pcm: Vec<u8> = Vec::new();
    for seg in &segments {
        pcm.extend_from_slice(
            &engine
                .decode(seg)
                .map_err(|e| format!("decode audio: {e}"))?,
        );
    }
    if pcm.is_empty() {
        return Err("no audio to export".into());
    }

    let Some(path) = app
        .dialog()
        .file()
        .add_filter("WAV-Audio", &["wav"])
        .set_file_name(&suggested_name)
        .blocking_save_file()
    else {
        return Ok(None); // user cancelled
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, wav_from_pcm(&pcm, sample_rate)).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Synthesize speech for `text` with the local TTS provider, returning the PCM
/// for the frontend to play. `voice_id` picks the voice; an unknown or `None`
/// value falls back to the provider's default voice. Errors when no local TTS is
/// available so the UI can fall back to system speech.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn tts_speak(
    app: AppHandle,
    text: String,
    voice_id: Option<String>,
    provider: Option<String>,
    speed: Option<f32>,
    expressiveness: Option<f32>,
    cadence: Option<f32>,
    sentence_silence: Option<f32>,
    intonation: Option<f32>,
    brightness: Option<f32>,
    emotion: Option<Vec<f32>>,
) -> Result<TtsAudio, String> {
    off_thread(app, move |state| {
        let tts = tts_for(state, provider.as_deref())
            .ok_or_else(|| "no local TTS provider".to_string())?;
        let request = TtsRequest {
            text,
            voice_id: voice_id
                .filter(|v| !v.is_empty())
                .or_else(|| tts.default_voice())
                .unwrap_or_default(),
            speed: speed.unwrap_or(1.0),
            expressiveness,
            cadence,
            sentence_silence,
            intonation,
            brightness,
            emotion,
        };
        let response = tts
            .run(request, &CancelToken::new())
            .map_err(|e| e.to_string())?;

        // f32 [-1,1] → 16-bit LE PCM → base64.
        let mut bytes = Vec::with_capacity(response.samples.len() * 2);
        for &s in &response.samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        Ok(TtsAudio {
            pcm: base64::engine::general_purpose::STANDARD.encode(&bytes),
            sample_rate: response.sample_rate,
        })
    })
    .await
}

/// The voices the local TTS offers, across every available backend, so the UI
/// can let the user switch backend and voice. Piper's bundled voices come first
/// (always available); the XTTS voices follow when the sidecar is configured
/// (listed statically, even before it has warmed up). Each voice carries its
/// `provider`, which the UI passes back to [`tts_speak`].
#[tauri::command(async)]
pub fn list_tts_voices(state: State<AppState>) -> Vec<TtsVoice> {
    let mut voices = slot_provider(&state.tts)
        .map(|tts| tts.voices())
        .unwrap_or_default();
    if state
        .xtts_paths
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
    {
        voices.extend(exoquill_ai::XttsTts::voices_static());
    }
    if let Some((_, _, voices_dir)) = state.zonos_paths.lock().ok().and_then(|g| g.clone()) {
        voices.extend(exoquill_ai::ZonosTts::voices_in_dir(&voices_dir));
    }
    if let Some((_, _, voices_dir)) = state.chatterbox_paths.lock().ok().and_then(|g| g.clone()) {
        voices.extend(exoquill_ai::ChatterboxTts::voices_in_dir(&voices_dir));
    }
    if let Some((_, _, voices_dir)) = state.qwen3_paths.lock().ok().and_then(|g| g.clone()) {
        voices.extend(exoquill_ai::Qwen3Tts::predefined_voices());
        voices.extend(exoquill_ai::Qwen3Tts::voices_in_dir(&voices_dir));
    }
    // Native Kokoro: list its loaded voices directly (no sidecar to query).
    #[cfg(feature = "kokoro")]
    if let Some(kokoro) = slot_provider(&state.kokoro) {
        voices.extend(kokoro.voices());
    }
    voices
}

/// Read-only summary of the provider behind an AI capability, for the settings /
/// about view (D5: surface model + voice license/status without a full model
/// manager yet).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub feature: String,
    pub provider_id: String,
    pub display_name: String,
    pub version: String,
    /// "ready" | "mock" | "missing_model" | "unavailable" | "fallback".
    pub status: String,
    pub runtime_license: String,
    pub source: Option<String>,
}

fn describe<P: Provider + ?Sized>(feature: &str, provider: &P) -> ModelInfo {
    let license = provider.license_info();
    let status = if provider.id().contains("mock") {
        "mock"
    } else {
        match provider.health_check() {
            Health::Ready => "ready",
            Health::MissingModel { .. } => "missing_model",
            Health::Unavailable { .. } => "unavailable",
        }
    };
    ModelInfo {
        feature: feature.into(),
        provider_id: provider.id().into(),
        display_name: provider.display_name().into(),
        version: provider.version().into(),
        status: status.into(),
        runtime_license: license.runtime_license,
        source: license.source,
    }
}

/// List the resolved AI providers with license + status for the settings view.
#[tauri::command]
pub async fn list_model_info(app: AppHandle) -> Vec<ModelInfo> {
    off_thread(app, move |state| -> Result<Vec<ModelInfo>, String> {
        let mut out = vec![
            describe("stt", state.stt.as_ref()),
            describe("ocr", state.ocr.as_ref()),
            describe("formatter", state.formatter.as_ref()),
        ];
        match slot_provider(&state.tts) {
            Some(tts) => out.push(describe("tts", tts.as_ref())),
            None => out.push(ModelInfo {
                feature: "tts".into(),
                provider_id: "tts.system".into(),
                display_name: "System speech (fallback)".into(),
                version: "-".into(),
                status: "fallback".into(),
                runtime_license: "OS".into(),
                source: None,
            }),
        }
        Ok(out)
    })
    .await
    .unwrap_or_default()
}
