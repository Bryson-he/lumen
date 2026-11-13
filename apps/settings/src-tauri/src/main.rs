// Hide the console window on release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod core;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // Must be the first plugin. Opening Lumen again (from the tray, the bar
        // or the Start menu) shows the window that is already open instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            core::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![core::core_call])
        .run(tauri::generate_context!())
        .expect("error while running the Lumen settings app");
}
