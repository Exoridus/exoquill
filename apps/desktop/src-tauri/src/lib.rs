mod jobs;
mod notes;

use std::sync::{Arc, Mutex};

use exoquill_ai::formatter::FormatterProvider;
use exoquill_ai::mock::MockFormatter;
use exoquill_core::{EventSink, JobQueue};
use exoquill_db::Database;
use jobs::TauriEventSink;
use notes::AppState;
use tauri::Manager;

/// Returns the ExoQuill core crate version.
#[tauri::command]
fn app_version() -> String {
    exoquill_core::version().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("exoquill.db");
            let db = Database::open(db_path.to_str().expect("data dir path is valid UTF-8"))
                .map_err(|e| format!("failed to open database at {db_path:?}: {e}"))?;

            let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink::new(app.handle().clone()));
            let jobs = JobQueue::new(sink);

            app.manage(AppState {
                db: Arc::new(Mutex::new(db)),
                jobs,
                formatter: Arc::new(MockFormatter) as Arc<dyn FormatterProvider>,
            });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
