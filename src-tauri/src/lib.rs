mod commands;
pub mod smartthings;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::st_status,
            commands::st_set_token,
            commands::st_list_devices,
            commands::st_select_device,
            commands::st_clear,
            commands::st_execute,
            commands::st_inputs,
            commands::st_channel,
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
