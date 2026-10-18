//! Plano B: controle pela nuvem via SmartThings API.
//!
//! Útil quando a TV recusa o pareamento local (WebSocket). O usuário cria um
//! token pessoal em <https://account.smartthings.com/tokens> com os escopos
//! `r:devices:*` e `x:devices:*`, adiciona a TV no app SmartThings e conecta
//! aqui. O controle passa a ser feito pelos comandos padrão de capacidade.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

const API: &str = "https://api.smartthings.com/v1";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct StConfig {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    /// capacidade → componente que a expõe (ex.: "switch" → "main")
    #[serde(default)]
    pub cap_components: std::collections::HashMap<String, String>,
    /// estado local do mudo (a leitura pela nuvem é inconsistente em TVs 2019)
    #[serde(default)]
    pub muted: Option<bool>,
    /// estado local de reprodução
    #[serde(default)]
    pub playing: Option<bool>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StDevice {
    pub device_id: String,
    pub label: String,
    pub device_type: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StStatus {
    pub has_token: bool,
    pub device_id: Option<String>,
    pub device_label: Option<String>,
    pub capabilities: Vec<String>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Falha ao preparar o cliente HTTP: {e}"))
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("smartthings.json"))
}

pub fn load(app: &AppHandle) -> StConfig {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, cfg: &StConfig) {
    if let Some(p) = path(app) {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(p, json);
        }
    }
}

pub fn status(app: &AppHandle) -> StStatus {
    let cfg = load(app);
    StStatus {
        has_token: cfg.token.is_some(),
        device_id: cfg.device_id,
        device_label: cfg.device_label,
        capabilities: cfg.capabilities,
    }
}

// ---------------------------------------------------------------------------
// Chamadas à API
// ---------------------------------------------------------------------------

fn capabilities_of(components: Option<&Vec<Value>>) -> Vec<String> {
    let mut caps: Vec<String> = Vec::new();
    if let Some(list) = components {
        for component in list {
            if let Some(items) = component.get("capabilities").and_then(|c| c.as_array()) {
                for item in items {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        if !caps.iter().any(|c| c == id) {
                            caps.push(id.to_string());
                        }
                    }
                }
            }
        }
    }
    caps
}

pub async fn list_devices(token: &str) -> Result<Vec<StDevice>, String> {
    let response = client()?
        .get(format!("{API}/devices"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Falha ao falar com o SmartThings: {e}"))?;

    if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
        return Err("Token inválido ou sem permissão (escopos r:devices:* e x:devices:*).".into());
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Resposta inesperada do SmartThings: {e}"))?;

    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut devices: Vec<StDevice> = items
        .iter()
        .map(|item| {
            let capabilities = capabilities_of(item.get("components").and_then(|c| c.as_array()));
            StDevice {
                device_id: item
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                label: item
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(sem nome)")
                    .to_string(),
                device_type: item
                    .get("deviceTypeName")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                capabilities,
            }
        })
        .filter(|d| !d.device_id.is_empty())
        .collect();

    // Preferir TVs: quem tem capacidade de canal ou mídia (se nada bater,
    // devolve tudo para o usuário escolher).
    let is_tv = |d: &StDevice| {
        d.capabilities.iter().any(|c| c == "tvChannel" || c == "mediaPlayback")
    };
    if devices.iter().any(is_tv) {
        devices.retain(is_tv);
    }

    devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    Ok(devices)
}

/// Busca detalhes do dispositivo: (label, capabilities, componentes com suas
/// capacidades — na ordem em que a API devolve).
pub async fn fetch_device(
    token: &str,
    device_id: &str,
) -> Result<(String, Vec<String>, Vec<(String, Vec<String>)>), String> {
    let response = client()?
        .get(format!("{API}/devices/{device_id}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Falha ao consultar o dispositivo: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "SmartThings respondeu {} ao consultar o dispositivo.",
            response.status()
        ));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("Resposta inesperada: {e}"))?;

    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("TV")
        .to_string();

    let components = value
        .get("components")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut detailed: Vec<(String, Vec<String>)> = Vec::new();
    let mut all_caps: Vec<String> = Vec::new();

    for component in components {
        let id = component
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();
        let caps = capabilities_of(Some(&vec![component.clone()]));
        for cap in &caps {
            if !all_caps.iter().any(|c| c == cap) {
                all_caps.push(cap.clone());
            }
        }
        detailed.push((id, caps));
    }

    Ok((label, all_caps, detailed))
}

pub async fn get_status_raw(
    token: &str,
    device_id: &str,
    component: &str,
    capability: &str,
    attribute: &str,
) -> Result<Option<Value>, String> {
    let response = client()?
        .get(format!(
            "{API}/devices/{device_id}/components/{component}/capabilities/{capability}/status"
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Falha ao ler o estado: {e}"))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let value: Value = response.json().await.unwrap_or(Value::Null);
    Ok(value
        .get(capability)
        .and_then(|c| c.get(attribute))
        .and_then(|a| a.get("value"))
        .cloned())
}

pub async fn get_status(
    token: &str,
    device_id: &str,
    component: &str,
    capability: &str,
    attribute: &str,
) -> Result<Option<String>, String> {
    Ok(get_status_raw(token, device_id, component, capability, attribute)
        .await?
        .and_then(|v| v.as_str().map(str::to_string)))
}

pub async fn send_command(
    token: &str,
    device_id: &str,
    component: &str,
    capability: &str,
    command: &str,
    arguments: Vec<Value>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "commands": [{
            "component": component,
            "capability": capability,
            "command": command,
            "arguments": arguments,
        }]
    });

    let response = client()?
        .post(format!("{API}/devices/{device_id}/commands"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Falha ao enviar o comando: {e}"))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    let detail = response.text().await.unwrap_or_default();
    Err(format!(
        "SmartThings recusou o comando ({status}). {}",
        if detail.is_empty() {
            String::new()
        } else if detail.len() > 300 {
            format!("{}…", &detail[..300])
        } else {
            detail
        }
    ))
}

// ---------------------------------------------------------------------------
// Execução de ações (mapeamento tecla → capacidade)
// ---------------------------------------------------------------------------

/// Envia uma tecla pelo canal de controle remoto do SmartThings
/// (`samsungvd.remoteControl.send`), que aceita o conjunto abaixo.
const REMOTE_KEYS: [&str; 17] = [
    "UP", "DOWN", "LEFT", "RIGHT", "OK", "BACK", "EXIT", "MENU", "HOME", "MUTE", "PLAY", "PAUSE",
    "STOP", "REWIND", "FF", "PLAY_BACK", "SOURCE",
];

fn remote_key_attempt(key: &str) -> Result<(&'static str, String, Vec<Value>), String> {
    if !REMOTE_KEYS.contains(&key) {
        return Err(format!("A tecla \"{key}\" não é aceita pelo modo nuvem."));
    }
    Ok((
        "samsungvd.remoteControl",
        "send".to_string(),
        vec![
            Value::String(key.to_string()),
            Value::String("PRESS_AND_RELEASED".to_string()),
        ],
    ))
}

pub async fn execute(app: &AppHandle, action: &str, arg: Option<String>) -> Result<(), String> {
    let cfg = load(app);
    let token = cfg
        .token
        .clone()
        .ok_or("Modo nuvem sem token configurado")?;
    let device_id = cfg
        .device_id
        .clone()
        .ok_or("Nenhuma TV selecionada no modo nuvem")?;

    let has = |cap: &str| cfg.capabilities.iter().any(|c| c == cap);
    let component_for = |cap: &str| {
        cfg.cap_components
            .get(cap)
            .cloned()
            .or_else(|| cfg.components.first().cloned())
            .unwrap_or_else(|| "main".to_string())
    };

    // Estado previsto após o comando (a leitura de status pela nuvem é
    // inconsistente em TVs 2019; mantemos o estado local como reforço).
    let mut new_muted: Option<bool> = None;
    let mut new_playing: Option<bool> = None;

    // Resolve a capacidade, o comando e os argumentos de cada ação.
    let (capability, command, arguments): (&str, String, Vec<Value>) = match action {
        "power_on" | "power_off" => {
            if !has("switch") {
                return Err("A TV não expõe controle de energia no SmartThings.".into());
            }
            let cmd = if action == "power_on" { "on" } else { "off" };
            ("switch", cmd.to_string(), vec![])
        }

        "power_toggle" => {
            if !has("switch") {
                return Err("A TV não expõe controle de energia no SmartThings.".into());
            }
            let component = component_for("switch");
            let state = get_status(&token, &device_id, &component, "switch", "switch").await?;
            let cmd = if state.as_deref() == Some("on") { "off" } else { "on" };
            ("switch", cmd.to_string(), vec![])
        }

        "volume_up" | "volume_down" => {
            if !has("audioVolume") {
                return Err("Volume não disponível no modo nuvem para esta TV.".into());
            }
            let cmd = if action == "volume_up" { "volumeUp" } else { "volumeDown" };
            ("audioVolume", cmd.to_string(), vec![])
        }

        "mute_toggle" => {
            if has("samsungvd.remoteControl") {
                // tecla MUTE real do controle da TV (alterna)
                remote_key_attempt("MUTE")?
            } else if has("audioMute") || has("audioVolume") {
                let component = component_for("audioMute");
                let state =
                    get_status(&token, &device_id, &component, "audioMute", "mute").await?;
                let muted = match state.as_deref() {
                    Some("muted") | Some("mute") | Some("on") | Some("true") => true,
                    Some("unmuted") | Some("unmute") | Some("off") | Some("false") => false,
                    _ => cfg.muted.unwrap_or(false),
                };
                new_muted = Some(!muted);
                let cmd = if muted { "unmute" } else { "mute" };
                ("audioMute", cmd.to_string(), vec![])
            } else {
                return Err("Mudo não disponível no modo nuvem para esta TV.".into());
            }
        }

        "channel_up" | "channel_down" => {
            if !has("tvChannel") {
                return Err("Troca de canal não disponível no modo nuvem para esta TV.".into());
            }
            let cmd = if action == "channel_up" { "channelUp" } else { "channelDown" };
            ("tvChannel", cmd.to_string(), vec![])
        }

        "play_pause" => {
            if has("samsungvd.remoteControl") {
                // tecla PLAY_BACK real (alterna play/pause como no controle)
                remote_key_attempt("PLAY_BACK")?
            } else if has("mediaPlayback") {
                let component = component_for("mediaPlayback");
                let state =
                    get_status(&token, &device_id, &component, "mediaPlayback", "playbackStatus")
                        .await?;
                let playing = match state.as_deref() {
                    Some("play") | Some("playing") | Some("fast forward")
                    | Some("fast forwarding") => true,
                    Some("pause") | Some("paused") | Some("stop") | Some("stopped")
                    | Some("rewind") | Some("rewinding") => false,
                    _ => cfg.playing.unwrap_or(false),
                };
                new_playing = Some(!playing);
                let cmd = if playing { "pause" } else { "play" };
                ("mediaPlayback", cmd.to_string(), vec![])
            } else {
                return Err("Play/Pause não disponível no modo nuvem para esta TV.".into());
            }
        }

        "remote_key" => {
            if !has("samsungvd.remoteControl") {
                return Err("Esta TV não aceita teclas de navegação pela nuvem.".into());
            }
            let key = arg.ok_or("Tecla ausente para o comando de navegação")?;
            remote_key_attempt(key.trim().to_uppercase().as_str())?
        }

        "rewind" | "fast_forward" => {
            if !has("mediaPlayback") {
                return Err("Mídia não disponível no modo nuvem para esta TV.".into());
            }
            let cmd = if action == "rewind" { "rewind" } else { "fastForward" };
            ("mediaPlayback", cmd.to_string(), vec![])
        }

        "queue_next" => ("mediaTrackControl", "trackNext".to_string(), vec![]),

        "cycle_input" => {
            if has("samsungvd.mediaInputSource") {
                // Variante Samsung: traz a lista real de entradas (com os nomes
                // configurados para cada HDMI, ex.: "XBOX One").
                let component = component_for("samsungvd.mediaInputSource");
                let raw = get_status_raw(
                    &token,
                    &device_id,
                    &component,
                    "samsungvd.mediaInputSource",
                    "supportedInputSourcesMap",
                )
                .await?;

                let list: Vec<String> = raw
                    .and_then(|v| serde_json::from_value::<Vec<Value>>(v).ok())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                item.get("id").and_then(|v| v.as_str()).map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if list.is_empty() {
                    return Err("A TV não informou as entradas disponíveis.".into());
                }

                let current = get_status(
                    &token,
                    &device_id,
                    &component,
                    "samsungvd.mediaInputSource",
                    "inputSource",
                )
                .await?;

                let idx = current
                    .as_ref()
                    .and_then(|c| list.iter().position(|x| x == c))
                    .map(|i| (i + 1) % list.len())
                    .unwrap_or(0);

                let target = list[idx].clone();
                (
                    "samsungvd.mediaInputSource",
                    "setInputSource".to_string(),
                    vec![Value::String(target)],
                )
            } else if has("mediaInputSource") {
                let component = component_for("mediaInputSource");
                let current =
                    get_status(&token, &device_id, &component, "mediaInputSource", "inputSource")
                        .await?;
                let supported = get_status(
                    &token,
                    &device_id,
                    &component,
                    "mediaInputSource",
                    "supportedInputSources",
                )
                .await?;
                // supportedInputSources pode vir como lista JSON ou string
                // separada por vírgulas, dependendo da TV.
                let list: Vec<String> = supported
                    .as_deref()
                    .map(|s| {
                        s.split(',')
                            .map(|x| x.trim().trim_matches('"').trim_matches('[').trim_matches(']'))
                            .filter(|x| !x.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let next = if list.is_empty() {
                    None
                } else {
                    let idx = current
                        .as_ref()
                        .and_then(|c| list.iter().position(|x| x == c))
                        .map(|i| (i + 1) % list.len())
                        .unwrap_or(0);
                    list.get(idx).cloned()
                };
                let target = next.ok_or("A TV não informou as entradas disponíveis.")?;
                (
                    "mediaInputSource",
                    "setInputSource".to_string(),
                    vec![Value::String(target)],
                )
            } else {
                return Err("Troca de entrada não disponível no modo nuvem para esta TV.".into());
            }
        }

        "launch_app" => {
            let app_id = arg.ok_or("appId ausente para abrir o app")?;
            (
                "custom.launchapp",
                "launchApp".to_string(),
                vec![Value::String(app_id)],
            )
        }

        other => return Err(format!("Ação \"{other}\" não suportada no modo nuvem.")),
    };

    // Envia o comando; se a TV recusar, tenta variações equivalentes
    // (ex.: setMute em vez de mute, setPlaybackStatus em vez de pause).
    let component = component_for(capability);
    let mut result = send_command(
        &token,
        &device_id,
        &component,
        capability,
        &command,
        arguments,
    )
    .await;

    if result.is_err() && capability == "audioMute" {
        let want_muted = command == "mute";
        result = send_command(
            &token,
            &device_id,
            &component,
            "audioMute",
            "setMute",
            vec![Value::Bool(want_muted)],
        )
        .await;

        if result.is_err() {
            result = send_command(&token, &device_id, &component, "audioVolume", &command, vec![])
                .await;
        }
    }

    if result.is_err()
        && capability == "mediaPlayback"
        && (command == "play" || command == "pause")
    {
        result = send_command(
            &token,
            &device_id,
            &component,
            "mediaPlayback",
            "setPlaybackStatus",
            vec![Value::String(command.clone())],
        )
        .await;
    }

    result?;

    if new_muted.is_some() || new_playing.is_some() {
        let mut updated = cfg;
        if let Some(m) = new_muted {
            updated.muted = Some(m);
        }
        if let Some(p) = new_playing {
            updated.playing = Some(p);
        }
        save(app, &updated);
    }

    Ok(())
}
