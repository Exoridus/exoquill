mod jobs;
mod notes;
mod tray;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_ai::mock::{MockFormatter, MockOcr};
use exoquill_ai::ocr::OcrProvider;
use exoquill_ai::provider::{Health, Provider};
use exoquill_ai::TesseractOcr;
use exoquill_core::{EventSink, JobQueue};
use exoquill_db::Database;
use jobs::TauriEventSink;
use notes::AppState;
use tauri::{App, Emitter, Manager};
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
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
            app.manage(AppState {
                db: Arc::new(Mutex::new(db)),
                jobs,
                formatter: Arc::new(MockFormatter) as Arc<dyn FormatterProvider>,
                ocr,
            });

            tray::setup_tray(app)?;
            app.global_shortcut()
                .register(tray::quick_note_shortcut())?;
            tray::setup_close_to_tray(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            notes::create_note,
            notes::get_note,
            notes::update_note,
            notes::delete_note,
            notes::list_notes,
            notes::search_notes,
            notes::resolve_target_note,
            jobs::format_note,
            jobs::cancel_job,
            jobs::list_jobs,
            jobs::run_ocr,
            jobs::format_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
