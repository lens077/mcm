#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod prefs;
mod selftest;
mod watch;

use commands::AppState;

fn main() {
    // Scripted scenario harness for platforms without a WebDriver (research R8).
    if std::env::args().any(|arg| arg == "--selftest") {
        selftest::main_selftest();
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::session_new,
            commands::session_state,
            commands::issues_get,
            commands::app_close_check,
            commands::outline_text_get,
            commands::outline_text_apply,
            commands::scene_get,
            commands::search,
            commands::edit_apply,
            commands::undo,
            commands::redo,
            commands::session_open,
            commands::session_save,
            commands::file_check_external,
            commands::prefs_get,
            commands::prefs_set,
            commands::export_precheck,
            commands::export_run,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MCM desktop app");
}
