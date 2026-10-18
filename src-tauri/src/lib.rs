mod commands;
mod device;
mod smartthings;
mod ssdp;
mod store;
mod tv;
mod wol;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(tv::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::discover_tvs,
            commands::probe_tv,
            commands::list_saved_devices,
            commands::forget_device,
            commands::wake_on_lan,
            commands::connect_tv,
            commands::disconnect_tv,
            commands::send_key,
            commands::send_text,
            commands::launch_app,
            commands::list_apps,
            commands::power_state,
            commands::st_status,
            commands::st_set_token,
            commands::st_list_devices,
            commands::st_select_device,
            commands::st_clear,
            commands::st_execute,
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
