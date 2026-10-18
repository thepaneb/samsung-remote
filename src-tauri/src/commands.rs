//! Comandos expostos ao frontend.

use std::time::Duration;

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::device::{self, TvDevice};
use crate::ssdp;
use crate::store::{self, StoredDevice};
use crate::tv::{self, AppState};
use crate::wol;

fn from_stored(stored: StoredDevice) -> TvDevice {
    TvDevice {
        ip: stored.ip,
        name: stored.name,
        model: stored.model.clone(),
        model_name: stored.model,
        mac: stored.mac,
        power_state: None,
        paired: stored.token.is_some(),
        token: stored.token,
        reachable: false,
    }
}

fn merge_stored(mut device: TvDevice, stored: Option<StoredDevice>) -> TvDevice {
    if let Some(s) = stored {
        if device.name.is_none() {
            device.name = s.name;
        }
        if device.mac.is_none() {
            device.mac = s.mac;
        }
        if device.model.is_none() {
            device.model_name = s.model.clone();
            device.model = s.model;
        }
        device.paired = s.token.is_some();
        if s.token.is_some() {
            device.token = s.token;
        }
    }
    device
}

fn to_stored(device: &TvDevice, token: Option<String>) -> StoredDevice {
    StoredDevice {
        ip: device.ip.clone(),
        mac: device.mac.clone(),
        name: device.name.clone(),
        model: device.model_name.clone().or_else(|| device.model.clone()),
        token,
    }
}

async fn send_raw(state: &AppState, message: String) -> Result<(), String> {
    let guard = state.session.lock().await;
    match guard.as_ref() {
        Some(session) => session
            .tx
            .send(message)
            .map_err(|_| "A conexão com a TV foi perdida".to_string()),
        None => Err("Nenhuma TV conectada".to_string()),
    }
}

async fn stop_session(app: &AppHandle, state: &AppState) {
    {
        let mut guard = state.session.lock().await;
        *guard = None; // descartar o sender encerra a tarefa da sessão
    }
    tv::emit_status(app, "disconnected", None, Some("Desconectado"), None);
}

// ---------------------------------------------------------------------------
// Descoberta
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn discover_tvs(app: AppHandle, timeout_ms: Option<u64>) -> Result<Vec<TvDevice>, String> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(4500).clamp(1000, 15000));
    let ips = ssdp::discover_ips(timeout).await;
    if ips.is_empty() {
        return Ok(Vec::new());
    }

    let probes: Vec<_> = ips
        .into_iter()
        .map(|ip| {
            let app = app.clone();
            async move {
                let stored = store::find(&app, &ip);
                let device = match device::fetch_device_info(&ip).await {
                    Some(d) => merge_stored(d, stored),
                    None => match stored {
                        Some(s) => from_stored(s),
                        None => TvDevice {
                            ip: ip.clone(),
                            reachable: false,
                            ..Default::default()
                        },
                    },
                };
                let _ = app.emit("tv://discovered", &device);
                device
            }
        })
        .collect();

    Ok(join_all(probes).await)
}

#[tauri::command]
pub async fn probe_tv(app: AppHandle, ip: String) -> Result<TvDevice, String> {
    let ip = ip.trim().to_string();
    if ip.is_empty() {
        return Err("Informe o endereço IP da TV".to_string());
    }

    match device::fetch_device_info(&ip).await {
        Some(device) => {
            let merged = merge_stored(device, store::find(&app, &ip));
            store::upsert(&app, to_stored(&merged, merged.token.clone()));
            let _ = app.emit("tv://discovered", &merged);
            Ok(merged)
        }
        None => Err(format!(
            "Nenhuma TV respondeu em {ip}. Confirme o IP e se a TV está ligada."
        )),
    }
}

#[tauri::command]
pub fn list_saved_devices(app: AppHandle) -> Vec<TvDevice> {
    store::all(&app).into_iter().map(from_stored).collect()
}

#[tauri::command]
pub fn forget_device(app: AppHandle, ip: String) -> Result<(), String> {
    store::remove(&app, &ip);
    Ok(())
}

// ---------------------------------------------------------------------------
// Ligar / conectar
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn wake_on_lan(mac: String) -> Result<(), String> {
    wol::wake(&mac).await
}

#[tauri::command]
pub async fn connect_tv(
    app: AppHandle,
    state: State<'_, AppState>,
    ip: String,
) -> Result<TvDevice, String> {
    let ip = ip.trim().to_string();
    tv::emit_status(&app, "connecting", Some(&ip), Some("Procurando a TV…"), None);

    // Encerra sessão anterior (o shell emite "disconnected" antes do "connecting")
    stop_session_tx(&state).await;

    let mut device = device::fetch_device_info(&ip).await.ok_or_else(|| {
        format!("Não foi possível falar com a TV em {ip}. Confirme que ela está ligada e na mesma rede.")
    })?;

    let stored = store::find(&app, &ip);
    if let Some(s) = &stored {
        if device.mac.is_none() {
            device.mac = s.mac.clone();
        }
    }

    let initial_token = stored
        .as_ref()
        .and_then(|s| s.token.clone())
        .or_else(|| device.token.clone());

    let connected = tv::establish(&app, &ip, initial_token).await?;

    let stored_device = to_stored(&device, connected.token.clone());
    store::upsert(&app, stored_device);

    device.token = connected.token.clone();
    device.paired = device.token.is_some();

    let session_id = tv::next_session_id();
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    {
        let mut guard = state.session.lock().await;
        *guard = Some(tv::Session { id: session_id, tx });
    }

    tauri::async_runtime::spawn(tv::run_session(
        app.clone(),
        connected.ws,
        rx,
        session_id,
        ip.clone(),
    ));

    tv::emit_status(&app, "connected", Some(&ip), None, Some(device.clone()));
    Ok(device)
}

async fn stop_session_tx(state: &AppState) {
    let mut guard = state.session.lock().await;
    *guard = None;
}

#[tauri::command]
pub async fn disconnect_tv(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop_session(&app, &state).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Comandos para a TV
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn send_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    send_raw(&state, tv::key_message(&key)).await
}

#[tauri::command]
pub async fn send_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    send_raw(&state, tv::text_start_message()).await?;
    send_raw(&state, tv::text_message(&text)).await
}

#[tauri::command]
pub async fn launch_app(
    state: State<'_, AppState>,
    app_id: String,
    app_type: Option<i64>,
) -> Result<(), String> {
    send_raw(&state, tv::launch_message(&app_id, app_type)).await
}

// ---------------------------------------------------------------------------
// Consultas HTTP
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_apps(
    app: AppHandle,
    state: State<'_, AppState>,
    ip: String,
) -> Result<Vec<tv::TvApp>, String> {
    // 1) Caminho preferido: canal WebSocket já autorizado (a TV responde com
    //    o evento `ed.installedApp.get`). Funciona mesmo quando a REST exige
    //    token, como acontece em vários modelos.
    let connected = state.session.lock().await.is_some();
    if connected {
        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<tv::TvApp>>();
        state.apps_waiters.lock().await.push(tx);

        if send_raw(&state, tv::apps_request_message()).await.is_ok() {
            match tokio::time::timeout(Duration::from_secs(8), rx).await {
                Ok(Ok(apps)) if !apps.is_empty() => return Ok(apps),
                _ => state.apps_waiters.lock().await.clear(),
            }
        } else {
            state.apps_waiters.lock().await.clear();
        }
    }

    // 2) Fallback REST (com token, quando houver)
    let token = store::find(&app, &ip).and_then(|s| s.token);
    let client = device::http_client(8).ok_or("Falha ao preparar o cliente HTTP")?;
    let token_query = token.map(|t| format!("?token={t}")).unwrap_or_default();

    for url in [
        format!("http://{ip}:8001/api/v2/applications{token_query}"),
        format!("http://{ip}:8000/api/v2/applications{token_query}"),
        format!("https://{ip}:8002/api/v2/applications{token_query}"),
    ] {
        let Ok(response) = client.get(&url).send().await else {
            continue;
        };
        let Ok(value) = response.json::<serde_json::Value>().await else {
            continue;
        };
        let apps = tv::parse_apps(&value);
        if !apps.is_empty() {
            return Ok(apps);
        }
    }

    Err("Não foi possível obter a lista de apps da TV.".to_string())
}

#[tauri::command]
pub async fn power_state(ip: String) -> Result<Option<String>, String> {
    match device::fetch_device_info(&ip).await {
        Some(d) => Ok(d.power_state),
        None => Err("TV inacessível".to_string()),
    }
}
