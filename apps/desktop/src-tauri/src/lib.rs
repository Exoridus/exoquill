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
        .invoke_handler(tauri::generate_handler![greet, app_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
