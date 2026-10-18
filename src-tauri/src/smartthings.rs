//! Controle de TVs Samsung via SmartThings Cloud.
//!
//! O usuário cria um token pessoal em <https://account.smartthings.com/tokens>
//! (escopos `r:devices:*` e `x:devices:*`), adiciona a TV no app SmartThings e
//! conecta aqui. O controle usa os comandos de capacidade do dispositivo,
//! incluindo as capacidades Samsung (`samsungvd.remoteControl` para teclas de
//! navegação e `samsungvd.mediaInputSource` para troca direta de entrada).

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

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StInput {
    pub id: String,
    pub name: String,
    pub current: bool,
}

// ---------------------------------------------------------------------------
// Persistência
// ---------------------------------------------------------------------------

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
// Utilitários internos
// ---------------------------------------------------------------------------

fn has_cap(cfg: &StConfig, cap: &str) -> bool {
    cfg.capabilities.iter().any(|c| c == cap)
}

fn component_for(cfg: &StConfig, cap: &str) -> String {
    cfg.cap_components
        .get(cap)
        .cloned()
        .or_else(|| cfg.components.first().cloned())
        .unwrap_or_else(|| "main".to_string())
}

fn require_token_device(cfg: &StConfig) -> Result<(String, String), String> {
    let token = cfg
        .token
        .clone()
        .ok_or("Modo nuvem sem token configurado")?;
    let device_id = cfg
        .device_id
        .clone()
        .ok_or("Nenhuma TV selecionada no modo nuvem")?;
    Ok((token, device_id))
}

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

// ---------------------------------------------------------------------------
// Chamadas à API
// ---------------------------------------------------------------------------

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

    // Preferir TVs: quem tem capacidade de canal, mídia ou de TV Samsung.
    let is_tv = |d: &StDevice| {
        d.capabilities.iter().any(|c| {
            c == "tvChannel"
                || c == "mediaPlayback"
                || c == "samsungvd.remoteControl"
                || c == "samsungvd.mediaInputSource"
        })
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

    // A resposta traz os atributos no nível raiz ("{atributo}": {"value": ...});
    // por garantia, também aceitamos o formato aninhado por capacidade.
    Ok(value
        .get(attribute)
        .and_then(|a| a.get("value"))
        .cloned()
        .or_else(|| {
            value
                .get(capability)
                .and_then(|c| c.get(attribute))
                .and_then(|a| a.get("value"))
                .cloned()
        }))
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
// Entradas da TV
// ---------------------------------------------------------------------------

pub async fn list_inputs(app: &AppHandle) -> Result<Vec<StInput>, String> {
    let cfg = load(app);
    list_inputs_core(&cfg).await
}

/// Mesma lógica de `list_inputs`, sem depender do AppHandle (testável).
pub async fn list_inputs_core(cfg: &StConfig) -> Result<Vec<StInput>, String> {
    let (token, device_id) = require_token_device(cfg)?;

    // Variante Samsung: traz a lista real de entradas (com o nome configurado
    // em cada HDMI).
    if has_cap(cfg, "samsungvd.mediaInputSource") {
        let component = component_for(cfg, "samsungvd.mediaInputSource");
        let raw = get_status_raw(
            &token,
            &device_id,
            &component,
            "samsungvd.mediaInputSource",
            "supportedInputSourcesMap",
        )
        .await?;
        let current = get_status(
            &token,
            &device_id,
            &component,
            "samsungvd.mediaInputSource",
            "inputSource",
        )
        .await?;

        let inputs: Vec<StInput> = raw
            .as_ref()
            .and_then(|v| serde_json::from_value::<Vec<Value>>(v.clone()).ok())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let id = item.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or(id.as_str())
                            .to_string();
                        let current = current.as_deref() == Some(id.as_str());
                        Some(StInput { id, name, current })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !inputs.is_empty() {
            return Ok(inputs);
        }

        // Diagnóstico detalhado quando a leitura não devolveu o esperado.
        return Err(format!(
            "A TV não informou as entradas disponíveis. (componente: {component}; \
             supportedInputSourcesMap: {}; inputSource: {})",
            raw.as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "sem resposta".to_string()),
            current.unwrap_or_else(|| "sem resposta".to_string()),
        ));
    }

    // Fallback: capacidade padrão.
    if has_cap(cfg, "mediaInputSource") {
        let component = component_for(cfg, "mediaInputSource");
        let supported = get_status(
            &token,
            &device_id,
            &component,
            "mediaInputSource",
            "supportedInputSources",
        )
        .await?;
        let current = get_status(
            &token,
            &device_id,
            &component,
            "mediaInputSource",
            "inputSource",
        )
        .await?;

        let inputs: Vec<StInput> = supported
            .as_deref()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().trim_matches('"').trim_matches('[').trim_matches(']'))
                    .filter(|x| !x.is_empty())
                    .map(|id| StInput {
                        id: id.to_string(),
                        name: id.to_string(),
                        current: current.as_deref() == Some(id),
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !inputs.is_empty() {
            return Ok(inputs);
        }

        return Err(format!(
            "A TV não informou as entradas disponíveis. (componente: {component}; \
             supportedInputSources: {}; inputSource: {})",
            supported.unwrap_or_else(|| "sem resposta".to_string()),
            current.unwrap_or_else(|| "sem resposta".to_string()),
        ));
    }

    Err("A TV não expõe troca de entrada no modo nuvem.".into())
}

// ---------------------------------------------------------------------------
// Execução de ações (tecla → capacidade)
// ---------------------------------------------------------------------------

/// Teclas aceitas por `samsungvd.remoteControl.send`.
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
    let (token, device_id) = require_token_device(&cfg)?;

    let (capability, command, arguments): (&str, String, Vec<Value>) = match action {
        "power_on" | "power_off" => {
            if !has_cap(&cfg, "switch") {
                return Err("A TV não expõe controle de energia no SmartThings.".into());
            }
            let cmd = if action == "power_on" { "on" } else { "off" };
            ("switch", cmd.to_string(), vec![])
        }

        "power_toggle" => {
            if !has_cap(&cfg, "switch") {
                return Err("A TV não expõe controle de energia no SmartThings.".into());
            }
            let component = component_for(&cfg, "switch");
            let state = get_status(&token, &device_id, &component, "switch", "switch").await?;
            let cmd = if state.as_deref() == Some("on") { "off" } else { "on" };
            ("switch", cmd.to_string(), vec![])
        }

        "volume_up" | "volume_down" => {
            if !has_cap(&cfg, "audioVolume") {
                return Err("Volume não disponível no modo nuvem para esta TV.".into());
            }
            let cmd = if action == "volume_up" { "volumeUp" } else { "volumeDown" };
            ("audioVolume", cmd.to_string(), vec![])
        }

        "mute" => {
            if has_cap(&cfg, "samsungvd.remoteControl") {
                remote_key_attempt("MUTE")?
            } else if has_cap(&cfg, "audioMute") {
                let component = component_for(&cfg, "audioMute");
                let state =
                    get_status(&token, &device_id, &component, "audioMute", "mute").await?;
                let muted = matches!(
                    state.as_deref(),
                    Some("muted") | Some("mute") | Some("on") | Some("true")
                );
                let cmd = if muted { "unmute" } else { "mute" };
                ("audioMute", cmd.to_string(), vec![])
            } else {
                return Err("Mudo não disponível no modo nuvem para esta TV.".into());
            }
        }

        "channel_up" | "channel_down" => {
            if !has_cap(&cfg, "tvChannel") {
                return Err("Troca de canal não disponível no modo nuvem para esta TV.".into());
            }
            let cmd = if action == "channel_up" { "channelUp" } else { "channelDown" };
            ("tvChannel", cmd.to_string(), vec![])
        }

        "play_pause" => {
            if has_cap(&cfg, "samsungvd.remoteControl") {
                remote_key_attempt("PLAY_BACK")?
            } else if has_cap(&cfg, "mediaPlayback") {
                let component = component_for(&cfg, "mediaPlayback");
                let state = get_status(
                    &token,
                    &device_id,
                    &component,
                    "mediaPlayback",
                    "playbackStatus",
                )
                .await?;
                let playing = matches!(
                    state.as_deref(),
                    Some("play") | Some("playing") | Some("fast forward") | Some("fast forwarding")
                );
                let cmd = if playing { "pause" } else { "play" };
                ("mediaPlayback", cmd.to_string(), vec![])
            } else {
                return Err("Play/Pause não disponível no modo nuvem para esta TV.".into());
            }
        }

        "rewind" | "fast_forward" => {
            if !has_cap(&cfg, "mediaPlayback") && !has_cap(&cfg, "samsungvd.remoteControl") {
                return Err("Mídia não disponível no modo nuvem para esta TV.".into());
            }
            if has_cap(&cfg, "mediaPlayback") {
                let cmd = if action == "rewind" { "rewind" } else { "fastForward" };
                ("mediaPlayback", cmd.to_string(), vec![])
            } else {
                remote_key_attempt(if action == "rewind" { "REWIND" } else { "FF" })?
            }
        }

        "set_input" => {
            let id = arg.ok_or("Entrada ausente para a troca de fonte")?;
            let capability = if has_cap(&cfg, "samsungvd.mediaInputSource") {
                "samsungvd.mediaInputSource"
            } else if has_cap(&cfg, "mediaInputSource") {
                "mediaInputSource"
            } else {
                return Err("Troca de entrada não disponível no modo nuvem para esta TV.".into());
            };
            (capability, "setInputSource".to_string(), vec![Value::String(id)])
        }

        "source_key" => {
            if !has_cap(&cfg, "samsungvd.remoteControl") {
                return Err("Esta TV não aceita a tecla Fonte pela nuvem.".into());
            }
            remote_key_attempt("SOURCE")?
        }

        "remote_key" => {
            if !has_cap(&cfg, "samsungvd.remoteControl") {
                return Err("Esta TV não aceita teclas de navegação pela nuvem.".into());
            }
            let key = arg.ok_or("Tecla ausente para o comando de navegação")?;
            remote_key_attempt(key.trim().to_uppercase().as_str())?
        }

        "launch_app" => {
            let app_id = arg.ok_or("appId ausente para abrir o app")?;
            (
                "custom.launchapp",
                "launchApp".to_string(),
                vec![Value::String(app_id)],
            )
        }

        other => return Err(format!("Ação \"{other}\" não suportada.")),
    };

    let component = component_for(&cfg, capability);
    send_command(
        &token,
        &device_id,
        &component,
        capability,
        &command,
        arguments,
    )
    .await
}
