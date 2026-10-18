//! Comandos expostos ao frontend.

use tauri::AppHandle;

use crate::smartthings::{self, StDevice, StInput, StStatus};

#[tauri::command]
pub fn st_status(app: AppHandle) -> StStatus {
    smartthings::status(&app)
}

#[tauri::command]
pub async fn st_set_token(app: AppHandle, token: String) -> Result<Vec<StDevice>, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("Informe o token pessoal do SmartThings".to_string());
    }
    let devices = smartthings::list_devices(&token).await?;

    let mut cfg = smartthings::load(&app);
    cfg.token = Some(token);
    cfg.device_id = None;
    cfg.device_label = None;
    cfg.capabilities.clear();
    cfg.components.clear();
    cfg.cap_components.clear();
    smartthings::save(&app, &cfg);

    Ok(devices)
}

#[tauri::command]
pub async fn st_list_devices(app: AppHandle) -> Result<Vec<StDevice>, String> {
    let cfg = smartthings::load(&app);
    let token = cfg
        .token
        .ok_or("Configure o token do SmartThings primeiro")?;
    smartthings::list_devices(&token).await
}

#[tauri::command]
pub async fn st_select_device(app: AppHandle, device_id: String) -> Result<StStatus, String> {
    let mut cfg = smartthings::load(&app);
    let token = cfg
        .token
        .clone()
        .ok_or("Configure o token do SmartThings primeiro")?;

    let (label, capabilities, components) = smartthings::fetch_device(&token, &device_id).await?;

    // Mapeia cada capacidade ao componente que a expõe, preferindo "main".
    let mut ordered = components.clone();
    ordered.sort_by_key(|(id, _)| if id == "main" { 0 } else { 1 });

    let mut component_ids: Vec<String> = Vec::new();
    let mut cap_components = std::collections::HashMap::new();
    for (id, caps) in &ordered {
        if !component_ids.contains(id) {
            component_ids.push(id.clone());
        }
        for cap in caps {
            cap_components
                .entry(cap.clone())
                .or_insert_with(|| id.clone());
        }
    }

    cfg.device_id = Some(device_id);
    cfg.device_label = Some(label);
    cfg.capabilities = capabilities;
    cfg.components = component_ids;
    cfg.cap_components = cap_components;
    smartthings::save(&app, &cfg);

    Ok(smartthings::status(&app))
}

#[tauri::command]
pub fn st_clear(app: AppHandle) -> Result<(), String> {
    smartthings::save(&app, &Default::default());
    Ok(())
}

#[tauri::command]
pub async fn st_execute(app: AppHandle, action: String, arg: Option<String>) -> Result<(), String> {
    smartthings::execute(&app, &action, arg).await
}

#[tauri::command]
pub async fn st_inputs(app: AppHandle) -> Result<Vec<StInput>, String> {
    smartthings::list_inputs(&app).await
}

#[tauri::command]
pub async fn st_channel(app: AppHandle) -> Result<smartthings::StChannel, String> {
    smartthings::channel_info(&app).await
}

#[tauri::command]
pub async fn st_modes(app: AppHandle) -> Result<smartthings::StModes, String> {
    smartthings::modes_info(&app).await
}

#[tauri::command]
pub async fn st_about(app: AppHandle) -> Result<smartthings::StAbout, String> {
    smartthings::about_info(&app).await
}
