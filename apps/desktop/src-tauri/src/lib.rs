mod dictation;
mod jobs;
mod models;
mod notes;
mod tray;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_ai::mock::{MockFormatter, MockOcr, MockSpeechToText};
use exoquill_ai::ocr::OcrProvider;
use exoquill_ai::provider::{Health, Provider};
use exoquill_ai::stt::SpeechToTextProvider;
use exoquill_ai::tts::TextToSpeechProvider;
use exoquill_ai::{LlamaFormatter, PiperTts, TesseractOcr, WhisperStt, XttsTts};
use exoquill_core::{EventSink, JobQueue};
use exoquill_db::Database;
use jobs::TauriEventSink;
use notes::AppState;
use tauri::{App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Returns the ExoQuill core crate version.
#[tauri::command]
fn app_version() -> String {
    exoquill_core::version().to_string()
}

/// Pick the OCR provider: real Tesseract when reachable, otherwise the mock.
/// Paths come from env vars (dev) or the bundled resource dir (release).
fn resolve_ocr_provider(app: &App) -> Arc<dyn OcrProvider> {
    let binary = std::env::var("EXOQUILL_TESSERACT")
        .unwrap_or_else(|_| r"C:\Program Files\Tesseract-OCR\tesseract.exe".to_string());
    let tessdata = std::env::var("EXOQUILL_TESSDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| app.path().resource_dir().ok().map(|d| d.join("tessdata")));
    let tesseract = TesseractOcr::new(binary, tessdata);
    if matches!(tesseract.health_check(), Health::Ready) {
        Arc::new(tesseract)
    } else {
        Arc::new(MockOcr)
    }
}

/// Pick the formatter provider: real llama.cpp + Qwen when reachable, else mock.
fn resolve_formatter_provider(app: &App) -> Arc<dyn FormatterProvider> {
    let resources = app.path().resource_dir().ok();
    let binary = std::env::var("EXOQUILL_LLAMA")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("llama/llama-completion.exe"))
        });
    let model = std::env::var("EXOQUILL_FORMATTER_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("models/qwen2.5-1.5b-instruct-q4_k_m.gguf"))
        });
    match (binary, model) {
        (Some(binary), Some(model)) => {
            let llama = LlamaFormatter::new(binary, model);
            if matches!(llama.health_check(), Health::Ready) {
                Arc::new(llama)
            } else {
                Arc::new(MockFormatter)
            }
        }
        _ => Arc::new(MockFormatter),
    }
}

/// Resolve the persistent llama-server binary + model. `llama-server.exe` sits
/// next to `llama-completion.exe` (bundled together) and shares the same Qwen
/// GGUF as the per-call formatter. `None` if either is missing — formatting then
/// runs via the per-call fallback in [`AppState`].
fn resolve_llama_server_paths(app: &App) -> Option<(PathBuf, PathBuf)> {
    let resources = app.path().resource_dir().ok();
    let cli = std::env::var("EXOQUILL_LLAMA")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("llama/llama-completion.exe"))
        })?;
    let server = cli.with_file_name("llama-server.exe");
    let model = std::env::var("EXOQUILL_FORMATTER_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("models/qwen2.5-1.5b-instruct-q4_k_m.gguf"))
        })?;
    (server.exists() && model.exists()).then_some((server, model))
}

/// Pick the STT provider: real whisper.cpp + ggml model when reachable, else
/// the mock (placeholder transcript). Paths come from env vars (dev) or the
/// bundled resource dir (release).
fn resolve_stt_provider(app: &App) -> Arc<dyn SpeechToTextProvider> {
    let resources = app.path().resource_dir().ok();
    let binary = std::env::var("EXOQUILL_WHISPER")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("whisper/whisper-cli.exe"))
        });
    let model = std::env::var("EXOQUILL_WHISPER_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("models/ggml-large-v3-turbo-q5_0.bin"))
        });
    match (binary, model) {
        (Some(binary), Some(model)) => {
            let whisper = WhisperStt::new(binary, model);
            if matches!(whisper.health_check(), Health::Ready) {
                Arc::new(whisper)
            } else {
                Arc::new(MockSpeechToText)
            }
        }
        _ => Arc::new(MockSpeechToText),
    }
}

/// Resolve the persistent whisper-server binary + model. `whisper-server.exe`
/// sits next to `whisper-cli.exe` (built/bundled together) and shares the same
/// ggml model as the per-call provider. `None` if either is missing — dictation
/// then runs without the server (no live partials), via the per-call fallback.
fn resolve_whisper_server_paths(app: &App) -> Option<(PathBuf, PathBuf)> {
    let resources = app.path().resource_dir().ok();
    let cli = std::env::var("EXOQUILL_WHISPER")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("whisper/whisper-cli.exe"))
        })?;
    let server = cli.with_file_name("whisper-server.exe");
    let model = std::env::var("EXOQUILL_WHISPER_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("models/ggml-large-v3-turbo-q5_0.bin"))
        })?;
    (server.exists() && model.exists()).then_some((server, model))
}

/// Resolve the Silero VAD ONNX model and point ONNX Runtime at its dynamic
/// library, for the `silero` feature. The model comes from `EXOQUILL_SILERO_MODEL`
/// or the bundled `models/silero_vad.onnx`; the runtime dll from `ORT_DYLIB_PATH`
/// (respected as-is), `EXOQUILL_ORT_DYLIB`, or the bundled
/// `runtimes/onnxruntime/onnxruntime.dll`. Returns `None` (→ energy gate) when the
/// model is absent. Fetch both with `scripts/fetch-silero.ps1`.
#[cfg(feature = "silero")]
fn resolve_silero_model_path(app: &App) -> Option<PathBuf> {
    let resources = app.path().resource_dir().ok();
    let model = std::env::var("EXOQUILL_SILERO_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| resources.as_ref().map(|d| d.join("models/silero_vad.onnx")))?;
    if !model.exists() {
        return None;
    }
    // ort (load-dynamic) finds onnxruntime via ORT_DYLIB_PATH; set it from our
    // bundled runtime if the caller hasn't already pointed it somewhere.
    if std::env::var_os("ORT_DYLIB_PATH").is_none() {
        let dll = std::env::var("EXOQUILL_ORT_DYLIB")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                resources
                    .as_ref()
                    .map(|d| d.join("runtimes/onnxruntime/onnxruntime.dll"))
            });
        if let Some(dll) = dll.filter(|p| p.exists()) {
            std::env::set_var("ORT_DYLIB_PATH", dll);
        }
    }
    Some(model)
}

/// Pick the TTS provider: real Piper when reachable, else `None` (the UI then
/// falls back to the webview's system speech synthesis). Every `*.onnx` in the
/// voices directory becomes a selectable voice. `EXOQUILL_PIPER_VOICE` still
/// names the default voice (dev); its parent directory is scanned for the rest.
/// For release the bundled `piper-voices/` resource dir is scanned instead, with
/// `de_DE-thorsten-medium` as the default.
fn resolve_tts_provider(app: &App) -> Option<Arc<dyn TextToSpeechProvider>> {
    // Experimental: prefer the XTTS-v2 sidecar when its URL is set and reachable
    // (multilingual DE/EN; non-commercial weights — test only, never bundled).
    // Falls through to Piper otherwise. Start it with scripts/xtts-server.py.
    if let Ok(url) = std::env::var("EXOQUILL_XTTS_URL") {
        if let Some(xtts) = XttsTts::connect(url) {
            return Some(Arc::new(xtts) as Arc<dyn TextToSpeechProvider>);
        }
    }

    let resources = app.path().resource_dir().ok();
    let binary = std::env::var("EXOQUILL_PIPER")
        .map(PathBuf::from)
        .ok()
        .or_else(|| resources.as_ref().map(|d| d.join("piper/piper.exe")))?;
    let default_voice = std::env::var("EXOQUILL_PIPER_VOICE")
        .map(PathBuf::from)
        .ok();
    let voices_dir = default_voice
        .as_ref()
        .and_then(|p| p.parent().map(PathBuf::from))
        .or_else(|| resources.as_ref().map(|d| d.join("piper-voices")))?;
    let default_id = default_voice
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "de_DE-thorsten-medium".to_string());
    let piper = PiperTts::discover(binary, voices_dir, default_id);
    matches!(piper.health_check(), Health::Ready)
        .then(|| Arc::new(piper) as Arc<dyn TextToSpeechProvider>)
}

/// Resolve the auto-spawn paths for the experimental XTTS sidecar: the venv
/// Python + `xtts-server.py`, from `EXOQUILL_XTTS_PYTHON` / `EXOQUILL_XTTS_SCRIPT`
/// (set by dev.ps1). `None` when not configured, or when `EXOQUILL_XTTS_URL`
/// points at an already-running server (that takes precedence). The XTTS weights
/// are non-commercial, so this is opt-in and never part of a bundled release.
fn resolve_xtts_paths(_app: &App) -> Option<(PathBuf, PathBuf)> {
    if std::env::var_os("EXOQUILL_XTTS_URL").is_some() {
        return None;
    }
    let python = std::env::var("EXOQUILL_XTTS_PYTHON")
        .map(PathBuf::from)
        .ok()?;
    let script = std::env::var("EXOQUILL_XTTS_SCRIPT")
        .map(PathBuf::from)
        .ok()?;
    (python.exists() && script.exists()).then_some((python, script))
}

/// Python + `zonos-server.py` + a reference-voice folder, from
/// `EXOQUILL_ZONOS_PYTHON` / `EXOQUILL_ZONOS_SCRIPT` / `EXOQUILL_ZONOS_VOICES`
/// (set by dev.ps1). `None` when not configured. Zonos weights are Apache-2.0,
/// but it needs a CUDA GPU, so it's opt-in via the env vars.
fn resolve_zonos_paths(_app: &App) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let python = std::env::var("EXOQUILL_ZONOS_PYTHON")
        .map(PathBuf::from)
        .ok()?;
    let script = std::env::var("EXOQUILL_ZONOS_SCRIPT")
        .map(PathBuf::from)
        .ok()?;
    let voices = std::env::var("EXOQUILL_ZONOS_VOICES")
        .map(PathBuf::from)
        .ok()?;
    (python.exists() && script.exists() && voices.exists()).then_some((python, script, voices))
}

/// Python + `chatterbox-server.py` + a reference-voice folder, from
/// `EXOQUILL_CHATTERBOX_PYTHON` / `EXOQUILL_CHATTERBOX_SCRIPT` / `EXOQUILL_CHATTERBOX_VOICES`
/// (set by dev.ps1). `None` when not configured. Chatterbox weights are MIT-licensed,
/// but it needs a CUDA GPU, so it's opt-in via the env vars.
fn resolve_chatterbox_paths(_app: &App) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let python = std::env::var("EXOQUILL_CHATTERBOX_PYTHON")
        .map(PathBuf::from)
        .ok()?;
    let script = std::env::var("EXOQUILL_CHATTERBOX_SCRIPT")
        .map(PathBuf::from)
        .ok()?;
    let voices = std::env::var("EXOQUILL_CHATTERBOX_VOICES")
        .map(PathBuf::from)
        .ok()?;
    (python.exists() && script.exists() && voices.exists()).then_some((python, script, voices))
}

/// Build the native Kokoro-82M TTS provider (ONNX Runtime, no sidecar). The model
/// and voices come from `EXOQUILL_KOKORO_MODEL` / `EXOQUILL_KOKORO_VOICES` (dev),
/// else the bundled `models/kokoro/model.onnx` and `models/kokoro/voices` resource
/// dir. Returns `None` (→ another TTS provider) when the assets, ONNX Runtime, or
/// espeak-ng binary are absent. Fetch model/voices via the manager (`tts-kokoro`).
///
/// Like the Silero gate, ONNX Runtime is linked dynamically (`ort` `load-dynamic`):
/// we point `ORT_DYLIB_PATH` at the bundled `onnxruntime.dll` when the caller
/// hasn't set it. Apache-2.0 weights, English, runs on CPU (or GPU via DirectML).
#[cfg(feature = "kokoro")]
fn resolve_kokoro_native(app: &App) -> Option<Arc<dyn TextToSpeechProvider>> {
    let resources = app.path().resource_dir().ok();
    let model = std::env::var("EXOQUILL_KOKORO_MODEL")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            resources
                .as_ref()
                .map(|d| d.join("models/kokoro/model.onnx"))
        })?;
    if !model.exists() {
        return None;
    }
    let voices = std::env::var("EXOQUILL_KOKORO_VOICES")
        .map(PathBuf::from)
        .ok()
        .or_else(|| resources.as_ref().map(|d| d.join("models/kokoro/voices")))?;
    if !voices.exists() {
        return None;
    }
    // ort (load-dynamic) finds onnxruntime via ORT_DYLIB_PATH; set it from our
    // bundled runtime if the caller hasn't already pointed it somewhere (mirrors
    // the Silero resolver).
    if std::env::var_os("ORT_DYLIB_PATH").is_none() {
        let dll = std::env::var("EXOQUILL_ORT_DYLIB")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                resources
                    .as_ref()
                    .map(|d| d.join("runtimes/onnxruntime/onnxruntime.dll"))
            });
        if let Some(dll) = dll.filter(|p| p.exists()) {
            std::env::set_var("ORT_DYLIB_PATH", dll);
        }
    }
    match exoquill_ai::KokoroTts::new(&model, &voices) {
        Ok(kokoro) => Some(Arc::new(kokoro) as Arc<dyn TextToSpeechProvider>),
        Err(error) => {
            eprintln!("kokoro native TTS unavailable: {error}");
            None
        }
    }
}

/// Open the region-OCR selection overlay: freeze the monitor under the cursor,
/// stash the screenshot in state, and show a borderless, always-on-top window
/// covering that monitor where the user drags a rectangle (snipping-tool style).
/// No-op if an overlay is already open. The webview routes by window label.
fn start_region_ocr(app: &AppHandle) {
    if app.get_webview_window("region-overlay").is_some() {
        return;
    }
    let cursor = match app.cursor_position() {
        Ok(pos) => pos,
        Err(error) => {
            let _ = app.emit("region-ocr-error", error.to_string());
            return;
        }
    };
    let shot = match exoquill_capture::capture_at_point(cursor.x as i32, cursor.y as i32) {
        Ok(shot) => shot,
        Err(error) => {
            let _ = app.emit("region-ocr-error", error);
            return;
        }
    };
    let (x, y, w, h) = (
        shot.logical_x,
        shot.logical_y,
        shot.logical_width,
        shot.logical_height,
    );
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut slot) = state.region_capture.lock() {
            *slot = Some(shot);
        }
    }
    if let Err(error) =
        WebviewWindowBuilder::new(app, "region-overlay", WebviewUrl::App("index.html".into()))
            .position(x, y)
            .inner_size(w, h)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(true)
            .build()
    {
        let _ = app.emit("region-ocr-error", error.to_string());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if *shortcut == tray::region_ocr_shortcut() {
                        start_region_ocr(app);
                    } else {
                        tray::show_main(app);
                        let _ = app.emit("quick-note", ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("exoquill.db");
            let db = Database::open(db_path.to_str().expect("data dir path is valid UTF-8"))
                .map_err(|e| format!("failed to open database at {db_path:?}: {e}"))?;

            let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink::new(app.handle().clone()));
            let jobs = JobQueue::new(sink);

            let ocr = resolve_ocr_provider(app);
            let formatter = resolve_formatter_provider(app);
            let llama_server_paths = resolve_llama_server_paths(app);
            let stt = resolve_stt_provider(app);
            let whisper_server_paths = resolve_whisper_server_paths(app);
            let tts = resolve_tts_provider(app);
            let xtts_paths = resolve_xtts_paths(app);
            let zonos_paths = resolve_zonos_paths(app);
            let chatterbox_paths = resolve_chatterbox_paths(app);
            app.manage(AppState {
                db: Arc::new(Mutex::new(db)),
                jobs,
                formatter,
                llama_server_paths,
                llama_server: Mutex::new(None),
                ocr,
                stt,
                whisper_server_paths,
                whisper_server: Mutex::new(None),
                tts,
                xtts_paths,
                xtts_server: Mutex::new(None),
                xtts_warming: std::sync::atomic::AtomicBool::new(false),
                zonos_paths,
                zonos_server: Mutex::new(None),
                zonos_warming: std::sync::atomic::AtomicBool::new(false),
                chatterbox_paths,
                chatterbox_server: Mutex::new(None),
                chatterbox_warming: std::sync::atomic::AtomicBool::new(false),
                #[cfg(feature = "kokoro")]
                kokoro: resolve_kokoro_native(app),
                read_cancel: Mutex::new(exoquill_core::CancelToken::new()),
                dictation: Mutex::new(None),
                region_capture: Mutex::new(None),
                #[cfg(feature = "silero")]
                silero_model_path: resolve_silero_model_path(app),
            });

            // TTS sidecars are NOT started here. Loading both XTTS and Zonos at
            // launch froze the UI (two heavy Python/CUDA model loads at once).
            // The UI calls `warm_tts(provider)` for the active backend instead, so
            // only one loads, on demand, at below-normal priority.

            tray::setup_tray(app)?;
            app.global_shortcut()
                .register(tray::quick_note_shortcut())?;
            app.global_shortcut()
                .register(tray::region_ocr_shortcut())?;
            tray::setup_close_to_tray(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            notes::create_note,
            notes::get_note,
            notes::update_note,
            notes::delete_note,
            notes::restore_note,
            notes::set_archived,
            notes::hard_delete_note,
            notes::purge_trash,
            notes::list_notes,
            notes::search_notes,
            notes::resolve_target_note,
            notes::list_note_events,
            notes::snapshot_note_version,
            notes::list_note_history,
            notes::restore_note_version,
            notes::export_note,
            notes::import_note,
            notes::export_markdown,
            jobs::format_note,
            jobs::cancel_job,
            jobs::list_jobs,
            jobs::run_ocr,
            jobs::ocr_image,
            jobs::get_region_capture,
            jobs::ocr_region,
            jobs::cancel_region_ocr,
            jobs::format_text,
            jobs::begin_read,
            jobs::prepare_speech,
            jobs::cancel_read,
            jobs::tts_speak,
            jobs::export_audio,
            jobs::warm_tts,
            jobs::ensure_tts_ready,
            jobs::list_tts_voices,
            jobs::list_model_info,
            models::list_catalog,
            models::install_model,
            models::delete_model,
            dictation::start_dictation,
            dictation::stop_dictation,
            dictation::list_capture_sources,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Kill the persistent whisper-server on quit (its Drop kills the
            // child); otherwise it would outlive the app.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Ok(mut server) = state.whisper_server.lock() {
                        let _ = server.take();
                    }
                    if let Ok(mut server) = state.llama_server.lock() {
                        let _ = server.take();
                    }
                    if let Ok(mut server) = state.xtts_server.lock() {
                        let _ = server.take();
                    }
                    if let Ok(mut server) = state.zonos_server.lock() {
                        let _ = server.take();
                    }
                    if let Ok(mut server) = state.chatterbox_server.lock() {
                        let _ = server.take();
                    }
                    // Kokoro is native (no child process) — nothing to kill here.
                }
            }
        });
}
