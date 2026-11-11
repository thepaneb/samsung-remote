mod commands;
pub mod discovery;
pub mod local;
pub mod smartthings;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(local::LocalState::new())
        .invoke_handler(tauri::generate_handler![
            commands::st_status,
            commands::st_set_token,
            commands::st_list_devices,
            commands::st_select_device,
            commands::st_clear,
            commands::st_execute,
            commands::st_inputs,
            commands::st_channel,
            commands::st_modes,
            commands::st_about,
            commands::local_discover,
            commands::local_status,
            commands::local_pair,
            commands::local_set_enabled,
            commands::local_forget,
            commands::local_key,
            commands::local_channel,
            commands::local_launch,
            commands::local_text,
            commands::local_apps,
        ])
        .setup(|app| {
            if let Ok(dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(dir);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
