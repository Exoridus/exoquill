//! Live dictation: a background worker captures the microphone, segments speech
//! on natural pauses and streams each finalized transcript to the webview as a
//! `dictation_segment` event, so text lands in the note in short pieces as the
//! user speaks. Audio capture and the heavy real-time loop run in Rust (cpal);
//! only model inference crosses into the Whisper sidecar (decisions D8). No raw
//! audio is ever persisted.
//!
//! Events emitted to the frontend:
//! - `dictation_started` — capture is live
//! - `dictation_segment` (String) — a finalized transcript chunk to insert
//! - `dictation_level` (f32) — input level in `[0, 1]` for the meter
//! - `dictation_error` (String) — a non-fatal error (e.g. no microphone)
//! - `dictation_stopped` — capture ended (worker exited)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use exoquill_ai::stt::{SpeechToTextProvider, SttRequest};
use exoquill_audio::{resample_to_16k, start_capture, Segmenter};
use exoquill_core::CancelToken;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::notes::AppState;

/// How often the input level is pushed to the meter while capturing.
const LEVEL_INTERVAL: Duration = Duration::from_millis(100);

/// Auto-stop the session after this long without any speech, so the user can
/// pause to think mid-dictation without the recording ending immediately.
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(60);

/// A selectable dictation source: a microphone, or an output device captured via
/// WASAPI loopback (`loopback = true`) to dictate from system audio.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSource {
    pub name: String,
    pub loopback: bool,
}

/// A running dictation session. Dropping is not enough to stop it — set `stop`
/// and join the worker (see [`stop_dictation`]).
pub struct DictationController {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Begin streaming dictation into the active note. No-op if already running.
#[tauri::command]
pub fn start_dictation(
    state: State<AppState>,
    app: AppHandle,
    device: Option<String>,
    language_mode: Option<String>,
    loopback: Option<bool>,
) -> Result<(), String> {
    let mut slot = state.dictation.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        return Ok(());
    }
    let stt = Arc::clone(&state.stt);
    let language = language_mode.unwrap_or_else(|| "de_en_terms".into());
    let loopback = loopback.unwrap_or(false);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);

    let handle = std::thread::spawn(move || {
        run(app, stt, device, language, loopback, worker_stop);
    });
    *slot = Some(DictationController { stop, handle });
    Ok(())
}

/// Stop the current dictation session, flushing any trailing utterance.
#[tauri::command]
pub fn stop_dictation(state: State<AppState>) -> Result<(), String> {
    let controller = state.dictation.lock().map_err(|e| e.to_string())?.take();
    if let Some(controller) = controller {
        controller.stop.store(true, Ordering::Relaxed);
        let _ = controller.handle.join();
    }
    Ok(())
}

/// The available dictation sources: microphones plus output devices that can be
/// captured via WASAPI loopback (to dictate from system audio).
#[tauri::command]
pub fn list_capture_sources() -> Vec<CaptureSource> {
    let mut sources: Vec<CaptureSource> = exoquill_audio::list_input_devices()
        .into_iter()
        .map(|name| CaptureSource {
            name,
            loopback: false,
        })
        .collect();
    sources.extend(
        exoquill_audio::list_output_devices()
            .into_iter()
            .map(|name| CaptureSource {
                name,
                loopback: true,
            }),
    );
    sources
}

/// Worker loop: capture → segment → transcribe → emit. Runs on its own thread
/// and owns the cpal stream (which is dropped when the loop ends).
fn run(
    app: AppHandle,
    stt: Arc<dyn SpeechToTextProvider>,
    device: Option<String>,
    language: String,
    loopback: bool,
    stop: Arc<AtomicBool>,
) {
    let capture = match start_capture(device.as_deref(), loopback) {
        Ok(capture) => capture,
        Err(error) => {
            let _ = app.emit("dictation_error", error);
            clear_session(&app);
            let _ = app.emit("dictation_stopped", ());
            return;
        }
    };
    let _ = app.emit("dictation_started", ());

    let rate = capture.sample_rate;
    let mut segmenter = Segmenter::new(rate);
    let cancel = CancelToken::new();
    let mut last_level = Instant::now();
    let mut last_voice = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        match capture.frames.recv_timeout(LEVEL_INTERVAL) {
            Ok(frame) => {
                if let Some(utterance) = segmenter.push(&frame) {
                    transcribe(&app, &stt, &language, rate, utterance, &cancel);
                }
                if segmenter.is_active() {
                    last_voice = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_level.elapsed() >= LEVEL_INTERVAL {
            let _ = app.emit("dictation_level", (segmenter.level() * 4.0).min(1.0));
            last_level = Instant::now();
        }
        // Auto-stop after a long silence so a thinking pause doesn't end the
        // session, but an abandoned one doesn't capture forever.
        if last_voice.elapsed() >= INACTIVITY_TIMEOUT {
            break;
        }
    }

    if let Some(utterance) = segmenter.flush() {
        transcribe(&app, &stt, &language, rate, utterance, &cancel);
    }
    drop(capture);
    clear_session(&app);
    let _ = app.emit("dictation_stopped", ());
}

/// Clear the session slot when the worker exits on its own (inactivity timeout
/// or device disconnect), so the next `start_dictation` isn't a no-op. The
/// explicit `stop_dictation` path takes the slot first, so this then finds none.
fn clear_session(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut slot) = state.dictation.lock() {
            let _ = slot.take();
        }
    }
}

/// Resample one utterance to 16 kHz, transcribe it and emit the text (or error).
fn transcribe(
    app: &AppHandle,
    stt: &Arc<dyn SpeechToTextProvider>,
    language: &str,
    rate: u32,
    utterance: Vec<f32>,
    cancel: &CancelToken,
) {
    let request = SttRequest {
        samples: resample_to_16k(&utterance, rate),
        sample_rate: 16_000,
        language_mode: language.to_string(),
        custom_terms: Vec::new(),
    };
    match stt.run(request, cancel) {
        Ok(response) => {
            let text = response.text.trim();
            if !text.is_empty() {
                let _ = app.emit("dictation_segment", text.to_string());
            }
        }
        Err(error) => {
            let _ = app.emit("dictation_error", error.to_string());
        }
    }
}
