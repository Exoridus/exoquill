mod notes;

use std::sync::Mutex;

use exoquill_db::Database;
use notes::AppState;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Returns the ExoQuill core crate version. Proves the workspace wiring:
/// frontend -> Tauri command -> `exoquill-core`.
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
            app.manage(AppState { db: Mutex::new(db) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            app_version,
            notes::create_note,
            notes::get_note,
            notes::update_note,
            notes::delete_note,
            notes::list_notes,
            notes::search_notes,
            notes::resolve_target_note,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
