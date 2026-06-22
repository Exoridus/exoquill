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
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use exoquill_ai::stt::{SpeechToTextProvider, SttRequest};
use exoquill_ai::WhisperServer;
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

/// While speech is in progress, re-transcribe the in-progress utterance this
/// often to emit a live partial transcript. Only used on the persistent
/// whisper-server path (the per-call fallback is too slow for partials).
const PARTIAL_INTERVAL: Duration = Duration::from_millis(700);

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
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn start_dictation(
    state: State<AppState>,
    app: AppHandle,
    device: Option<String>,
    language_mode: Option<String>,
    loopback: Option<bool>,
    auto_gain: Option<bool>,
    gain: Option<f32>,
    use_silero: Option<bool>,
) -> Result<(), String> {
    let mut slot = state.dictation.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        return Ok(());
    }
    let language = language_mode.unwrap_or_else(|| "de_en_terms".into());
    let loopback = loopback.unwrap_or(false);
    // Gain handling: mic + auto-gain → adaptive AGC (`None`); otherwise a fixed
    // multiplier. Loopback (system audio) is already line level, so it never gets
    // the AGC — only an optional manual trim.
    let gain_mode = if auto_gain.unwrap_or(true) && !loopback {
        None
    } else {
        Some(gain.unwrap_or(1.0))
    };
    let use_silero = use_silero.unwrap_or(true);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);

    // The worker resolves its STT provider itself (it may start the persistent
    // whisper-server, which can take a moment) so this command returns at once.
    let handle = std::thread::spawn(move || {
        run(
            app,
            device,
            language,
            loopback,
            gain_mode,
            use_silero,
            worker_stop,
        );
    });
    *slot = Some(DictationController { stop, handle });
    Ok(())
}

/// Stop the current dictation session, flushing any trailing utterance.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[allow(clippy::too_many_arguments)]
fn run(
    app: AppHandle,
    device: Option<String>,
    language: String,
    loopback: bool,
    gain: Option<f32>,
    use_silero: bool,
    stop: Arc<AtomicBool>,
) {
    // Start capture *before* resolving the STT provider so a cold whisper-server
    // start (model load + GPU init, several seconds) never swallows the opening
    // words. Frames captured during the warmup are segmented and queued, then
    // transcribed retroactively once the provider is ready.
    let capture = match start_capture(device.as_deref(), loopback, gain) {
        Ok(capture) => capture,
        Err(error) => {
            let _ = app.emit("dictation_error", error);
            clear_session(&app);
            let _ = app.emit("dictation_stopped", ());
            return;
        }
    };
    let _ = app.emit("dictation_started", ());

    // Resolve the STT provider on a side thread — `ensure_stt` may start the
    // persistent whisper-server and block for seconds, which must not stall
    // capture. The result (provider + whether it supports live partials) lands in
    // `stt_slot`; until then, finalized utterances wait in `pending`.
    type ResolvedStt = (Arc<dyn SpeechToTextProvider>, bool);
    let stt_slot: Arc<Mutex<Option<ResolvedStt>>> = Arc::new(Mutex::new(None));
    let warmup = {
        let app = app.clone();
        let stt_slot = Arc::clone(&stt_slot);
        std::thread::spawn(move || {
            let resolved = ensure_stt(&app);
            if let Ok(mut slot) = stt_slot.lock() {
                *slot = Some(resolved);
            }
        })
    };

    let rate = capture.sample_rate;
    let mut segmenter = Segmenter::new(rate);
    // With `--features silero`, `use_silero` set, and the model resolved, swap in
    // the Silero neural VAD; otherwise (disabled, or a load failure such as a
    // missing runtime) the energy gate above stays.
    #[cfg(feature = "silero")]
    if use_silero {
        if let Some(model) = app.state::<AppState>().silero_model_path.clone() {
            match exoquill_audio::SileroGate::new(&model) {
                Ok(gate) => segmenter = Segmenter::with_gate(rate, Box::new(gate)),
                Err(err) => eprintln!("Silero VAD unavailable ({err}); using energy gate"),
            }
        }
    }
    #[cfg(not(feature = "silero"))]
    let _ = use_silero; // only consulted by the silero feature
    let cancel = CancelToken::new();
    let mut last_level = Instant::now();
    let mut last_voice = Instant::now();
    let mut last_partial = Instant::now();
    // Utterances finalized before the provider was ready, transcribed in order
    // once it lands so the start of the session survives a cold server start.
    let mut pending: Vec<Vec<f32>> = Vec::new();
    let mut stt: Option<ResolvedStt> = None;
    // Stabilizes the live partial of the current utterance (reset on finalize).
    let mut stabilizer = PartialStabilizer::default();

    while !stop.load(Ordering::Relaxed) {
        // Adopt the provider as soon as the warmup finishes, flushing the backlog
        // that accumulated (in speech order) while it was loading.
        if stt.is_none() {
            if let Some(resolved) = stt_slot.lock().ok().and_then(|mut s| s.take()) {
                for utterance in pending.drain(..) {
                    transcribe(&app, &resolved.0, &language, rate, utterance, &cancel);
                }
                stt = Some(resolved);
            }
        }

        match capture.frames.recv_timeout(LEVEL_INTERVAL) {
            Ok(frame) => {
                if let Some(utterance) = segmenter.push(&frame) {
                    match stt.as_ref() {
                        Some((provider, _)) => {
                            transcribe(&app, provider, &language, rate, utterance, &cancel)
                        }
                        None => pending.push(utterance),
                    }
                    // The utterance is done; its partials shouldn't bleed into the
                    // next one's stable prefix.
                    stabilizer.reset();
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
        // Live partials: once the server is up and supports them, periodically
        // re-transcribe the in-progress utterance and stream it as ghost text.
        // Inline (blocking) is fine — the call is ~100 ms and frames just queue.
        if let Some((provider, true)) = stt.as_ref() {
            if segmenter.is_active() && last_partial.elapsed() >= PARTIAL_INTERVAL {
                emit_partial(
                    &app,
                    provider,
                    &language,
                    rate,
                    segmenter.utterance(),
                    &cancel,
                    &mut stabilizer,
                );
                last_partial = Instant::now();
            }
        }
        // Auto-stop after a long silence so a thinking pause doesn't end the
        // session, but an abandoned one doesn't capture forever.
        if last_voice.elapsed() >= INACTIVITY_TIMEOUT {
            break;
        }
    }

    // Flush any trailing utterance, queuing it if the provider is still loading.
    if let Some(utterance) = segmenter.flush() {
        match stt.as_ref() {
            Some((provider, _)) => transcribe(&app, provider, &language, rate, utterance, &cancel),
            None => pending.push(utterance),
        }
        stabilizer.reset();
    }
    // If the provider never landed but we have queued audio, wait for the warmup
    // so the opening words aren't dropped. With nothing queued, let it finish in
    // the background (the started server is cached in AppState for next time) so
    // stopping stays instant.
    if stt.is_none() && !pending.is_empty() {
        let _ = warmup.join();
        if let Some(resolved) = stt_slot.lock().ok().and_then(|mut s| s.take()) {
            stt = Some(resolved);
        }
    }
    if let Some((provider, _)) = stt.as_ref() {
        for utterance in pending.drain(..) {
            transcribe(&app, provider, &language, rate, utterance, &cancel);
        }
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

/// A live partial transcript split into a frozen prefix and a tentative tail by
/// [`PartialStabilizer`]. Serialized to the `dictation_partial` event; the UI
/// renders `stable` calmly and only lets `tail` flicker.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PartialTranscript {
    /// Words confirmed by two consecutive partials — frozen, won't change.
    stable: String,
    /// The still-tentative tail that may still be revised by later partials.
    tail: String,
}

/// Reduces ghost-text flicker by only revising the unstable tail of a live
/// partial (LocalAgreement-2): a word becomes permanently *committed* once two
/// consecutive partials agree on it, since whisper rarely changes a word it has
/// emitted identically twice as more audio arrives. Reset between utterances.
#[derive(Default)]
struct PartialStabilizer {
    /// The previous partial's words, to diff the next one against.
    prev: Vec<String>,
    /// Frozen prefix: words agreed by two consecutive partials. Only grows.
    committed: Vec<String>,
}

impl PartialStabilizer {
    /// Feed the newest full partial transcript; returns the committed prefix and
    /// the still-tentative tail.
    fn push(&mut self, text: &str) -> PartialTranscript {
        let words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
        // Leading words this partial shares with the previous one are agreed and
        // become committed (committed only grows, so a later reinterpretation of
        // a frozen word is ignored — the first agreement wins).
        let agreed = words
            .iter()
            .zip(&self.prev)
            .take_while(|(a, b)| a == b)
            .count();
        while self.committed.len() < agreed {
            self.committed.push(words[self.committed.len()].clone());
        }
        let split = self.committed.len().min(words.len());
        let tail = words[split..].join(" ");
        self.prev = words;
        PartialTranscript {
            stable: self.committed.join(" "),
            tail,
        }
    }

    /// Forget all state so the next utterance's partials start fresh.
    fn reset(&mut self) {
        self.prev.clear();
        self.committed.clear();
    }
}

/// Transcribe the in-progress utterance and emit it as a live partial transcript
/// (the UI shows it as ghost text and replaces it when the segment finalizes).
/// Stabilized via `stabilizer` so the already-settled prefix doesn't flicker.
/// Best-effort: a transient failure is swallowed so it never interrupts capture.
fn emit_partial(
    app: &AppHandle,
    stt: &Arc<dyn SpeechToTextProvider>,
    language: &str,
    rate: u32,
    buffer: &[f32],
    cancel: &CancelToken,
    stabilizer: &mut PartialStabilizer,
) {
    if buffer.is_empty() {
        return;
    }
    let request = SttRequest {
        samples: resample_to_16k(buffer, rate),
        sample_rate: 16_000,
        language_mode: language.to_string(),
        custom_terms: Vec::new(),
    };
    if let Ok(response) = stt.run(request, cancel) {
        let text = response.text.trim();
        if !text.is_empty() {
            let _ = app.emit("dictation_partial", stabilizer.push(text));
        }
    }
}

/// Resolve the STT provider for a session: the persistent whisper-server
/// (starting it on first use), which enables live partials, otherwise the
/// per-call fallback in [`AppState`] (`false` = no partials). A server-start
/// failure falls through silently to the fallback — dictation still works.
fn ensure_stt(app: &AppHandle) -> (Arc<dyn SpeechToTextProvider>, bool) {
    let state = app.state::<AppState>();
    if let Some((binary, model)) = state.whisper_server_paths.clone() {
        let mut slot = match state.whisper_server.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.is_none() {
            if let Ok(server) = WhisperServer::start(&binary, &model) {
                *slot = Some(server);
            }
        }
        if let Some(server) = slot.as_ref() {
            if let Ok(client) = server.client() {
                return (Arc::new(client) as Arc<dyn SpeechToTextProvider>, true);
            }
        }
    }
    (Arc::clone(&state.stt), false)
}

#[cfg(test)]
mod tests {
    use super::PartialStabilizer;

    #[test]
    fn commits_words_agreed_by_two_partials() {
        let mut s = PartialStabilizer::default();
        let p = s.push("der");
        assert_eq!((p.stable.as_str(), p.tail.as_str()), ("", "der"));
        let p = s.push("der hund");
        assert_eq!((p.stable.as_str(), p.tail.as_str()), ("der", "hund"));
        let p = s.push("der hund bellt");
        assert_eq!((p.stable.as_str(), p.tail.as_str()), ("der hund", "bellt"));
    }

    #[test]
    fn committed_prefix_is_frozen_against_later_revision() {
        let mut s = PartialStabilizer::default();
        s.push("der");
        s.push("der hund"); // "der" is now agreed twice → committed.
                            // A later partial reinterprets the first word, but it's already frozen.
        let p = s.push("den hund bellt");
        assert_eq!((p.stable.as_str(), p.tail.as_str()), ("der", "hund bellt"));
    }

    #[test]
    fn reset_starts_the_next_utterance_fresh() {
        let mut s = PartialStabilizer::default();
        s.push("eins");
        s.push("eins zwei");
        s.reset();
        let p = s.push("drei");
        assert_eq!((p.stable.as_str(), p.tail.as_str()), ("", "drei"));
    }
}
