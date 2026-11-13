// Hide the console window on release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            core::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![core::core_call])
        .run(tauri::generate_context!())
        .expect("error while running the Lumen settings app");
}
