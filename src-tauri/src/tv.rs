//! Canal de controle via WebSocket da Samsung.
//!
//! TVs Tizen (2016+) expõem `ws://ip:8001/api/v2/channels/samsung.remote.control`
//! (e, em modelos mais novos, `wss://ip:8002`). O primeiro pareamento mostra um
//! popup na TV; ao aceitar, a TV devolve um token que guardamos para as próximas
//! conexões.

use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use native_tls::TlsConnector;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{
    connect_async, connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};

use std::sync::atomic::{AtomicU64, Ordering};

use crate::device::TvDevice;
use crate::store;

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

static SESSION_IDS: AtomicU64 = AtomicU64::new(1);

pub fn next_session_id() -> u64 {
    SESSION_IDS.fetch_add(1, Ordering::Relaxed)
}

pub struct Session {
    pub id: u64,
    pub tx: mpsc::UnboundedSender<String>,
}

#[derive(Default)]
pub struct AppState {
    pub session: Mutex<Option<Session>>,
    /// Aguardando a resposta `ed.installedApp.get` da TV.
    pub apps_waiters: Mutex<Vec<tokio::sync::oneshot::Sender<Vec<TvApp>>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: &'static str,
    pub ip: Option<String>,
    pub message: Option<String>,
    pub device: Option<TvDevice>,
}

pub fn emit_status(
    app: &AppHandle,
    state: &'static str,
    ip: Option<&str>,
    message: Option<&str>,
    device: Option<TvDevice>,
) {
    let _ = app.emit(
        "tv://status",
        Status {
            state,
            ip: ip.map(str::to_string),
            message: message.map(str::to_string),
            device,
        },
    );
}

// ---------------------------------------------------------------------------
// Mensagens enviadas à TV
// ---------------------------------------------------------------------------

pub fn key_message(key: &str) -> String {
    serde_json::json!({
        "method": "ms.remote.control",
        "params": {
            "Cmd": "Click",
            "DataOfCmd": key,
            "Option": "false",
            "TypeOfRemote": "SendRemoteKey"
        }
    })
    .to_string()
}

pub fn text_start_message() -> String {
    // Alguns modelos só aceitam SendInputString depois deste broadcast
    serde_json::json!({
        "method": "ms.channel.emit",
        "params": {
            "event": "custom.remote.textReceived",
            "to": "broadcast"
        }
    })
    .to_string()
}

pub fn text_message(text: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    serde_json::json!({
        "method": "ms.remote.control",
        "params": {
            "Cmd": encoded,
            "DataOfCmd": "base64",
            "TypeOfRemote": "SendInputString"
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Apps (lista via WebSocket autorizado ou REST com token)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TvApp {
    pub app_id: String,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub app_type: Option<i64>,
}

/// Requisição da lista de apps pelo canal WebSocket (a TV responde com um
/// evento `ed.installedApp.get`, tratado em `run_session`).
pub fn apps_request_message() -> String {
    serde_json::json!({
        "method": "ms.channel.emit",
        "params": {
            "event": "ed.installedApp.get",
            "to": "host"
        }
    })
    .to_string()
}

/// Extrai a lista de apps de um payload da TV (WebSocket ou REST).
pub fn parse_apps(value: &serde_json::Value) -> Vec<TvApp> {
    let entries = value
        .get("data")
        .and_then(|d| d.get("data"))
        .or_else(|| value.get("applications"))
        .or_else(|| value.get("data"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    entries
        .iter()
        .filter_map(|entry| {
            let app_id = entry.get("appId").and_then(|v| v.as_str())?.to_string();
            if app_id.trim().is_empty() {
                return None;
            }
            let name = entry
                .get("name")
                .or_else(|| entry.get("appName"))
                .and_then(|v| v.as_str())
                .unwrap_or(app_id.as_str())
                .to_string();
            let icon = entry
                .get("icon")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let app_type = entry.get("app_type").and_then(|v| v.as_i64());
            Some(TvApp {
                app_id,
                name,
                icon,
                app_type,
            })
        })
        .collect()
}

pub fn launch_message(app_id: &str, app_type: Option<i64>) -> String {
    // apps do tipo 2 usam DEEP_LINK; os demais, NATIVE_LAUNCH (regra da
    // comunidade samsungtvws). Sem a informação, DEEP_LINK é o padrão.
    let action = match app_type {
        Some(2) | None => "DEEP_LINK",
        Some(_) => "NATIVE_LAUNCH",
    };
    serde_json::json!({
        "method": "ms.channel.emit",
        "params": {
            "event": "ed.apps.launch",
            "to": "host",
            "data": {
                "appId": app_id,
                "action_type": action,
                "metaTag": ""
            }
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Conexão / handshake
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Handshake {
    Connected { token: Option<String> },
    Unauthorized,
}

pub struct Connected {
    pub ws: Ws,
    pub token: Option<String>,
}

async fn open(ip: &str, name_b64: &str, token: Option<&str>, tls: bool) -> Result<Ws, String> {
    let (scheme, port) = if tls { ("wss", 8002) } else { ("ws", 8001) };
    let token_query = token.map(|t| format!("&token={t}")).unwrap_or_default();
    let url = format!(
        "{scheme}://{ip}:{port}/api/v2/channels/samsung.remote.control?name={name_b64}{token_query}"
    );

    let timeout = Duration::from_secs(6);

    if tls {
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| format!("Falha ao preparar TLS: {e}"))?;
        let (ws, _) = tokio::time::timeout(
            timeout,
            connect_async_tls_with_config(url, None, false, Some(Connector::NativeTls(connector))),
        )
        .await
        .map_err(|_| "Tempo esgotado ao abrir o WebSocket seguro".to_string())?
        .map_err(|e| e.to_string())?;
        Ok(ws)
    } else {
        let (ws, _) = tokio::time::timeout(timeout, connect_async(url))
            .await
            .map_err(|_| "Tempo esgotado ao abrir o WebSocket".to_string())?
            .map_err(|e| e.to_string())?;
        Ok(ws)
    }
}

async fn handshake(
    app: &AppHandle,
    ip: &str,
    ws: &mut Ws,
    deadline: Duration,
) -> Result<Handshake, String> {
    let end = tokio::time::Instant::now() + deadline;
    let started = tokio::time::Instant::now();
    let mut notified_waiting = false;

    loop {
        let now = tokio::time::Instant::now();
        if now >= end {
            return Err("Tempo esgotado aguardando autorização na TV.".to_string());
        }

        if !notified_waiting && started.elapsed() > Duration::from_millis(1200) {
            notified_waiting = true;
            emit_status(
                app,
                "waiting",
                Some(ip),
                Some("Aguarde: se aparecer um aviso na TV, selecione \"Permitir\" com o controle físico."),
                None,
            );
        }

        let slice = std::cmp::min(end - now, Duration::from_secs(1));
        match tokio::time::timeout(slice, ws.next()).await {
            Err(_) => continue,
            Ok(None) => return Err("A TV encerrou a conexão.".to_string()),
            Ok(Some(Err(e))) => return Err(format!("Erro no canal de controle: {e}")),
            Ok(Some(Ok(Message::Text(txt)))) => {
                let value: serde_json::Value =
                    serde_json::from_str(txt.as_str()).unwrap_or(serde_json::Value::Null);
                match value.get("event").and_then(|e| e.as_str()).unwrap_or("") {
                    "ms.channel.connect" => {
                        let token = value
                            .get("data")
                            .and_then(|d| d.get("token"))
                            .and_then(|t| t.as_str())
                            .map(str::to_string);
                        return Ok(Handshake::Connected { token });
                    }
                    "ms.channel.unauthorized" => return Ok(Handshake::Unauthorized),
                    _ => continue,
                }
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                let _ = ws.send(Message::Pong(payload)).await;
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                return Err("A TV encerrou a conexão.".to_string())
            }
            Ok(Some(Ok(_))) => continue,
        }
    }
}

/// Abre o canal de controle e conclui o handshake, tentando primeiro
/// `ws://:8001` e depois `wss://:8002`. Se o token guardado for recusado,
/// limpa-o e tenta novamente sem token (novo pareamento na TV).
pub async fn establish(
    app: &AppHandle,
    ip: &str,
    initial_token: Option<String>,
) -> Result<Connected, String> {
    let name_b64 = base64::engine::general_purpose::STANDARD.encode("Controle Tauri");
    let mut token = initial_token.filter(|t| !t.trim().is_empty());
    let mut attempt = 0;

    loop {
        attempt += 1;

        let mut ws: Option<Ws> = None;
        let mut last_error: Option<String> = None;

        for tls in [false, true] {
            match open(ip, &name_b64, token.as_deref(), tls).await {
                Ok(socket) => {
                    ws = Some(socket);
                    break;
                }
                Err(e) => last_error = Some(e),
            }
        }

        let mut ws = ws.ok_or_else(|| {
            format!(
                "Não foi possível abrir o canal de controle com {ip}. \
                 Verifique se a TV está ligada, na mesma rede e com o IP correto. ({})",
                last_error.unwrap_or_default()
            )
        })?;

        emit_status(
            app,
            "connecting",
            Some(ip),
            Some("Canal aberto. Negociando acesso…"),
            None,
        );

        match handshake(app, ip, &mut ws, Duration::from_secs(60)).await? {
            Handshake::Connected { token: new_token } => {
                let final_token = new_token.or(token);
                return Ok(Connected {
                    ws,
                    token: final_token,
                });
            }
            Handshake::Unauthorized => {
                if attempt == 1 && token.is_some() {
                    // Token antigo inválido: esquece e tenta de novo (novo popup na TV)
                    store::clear_token(app, ip);
                    token = None;
                    continue;
                }
                return Err(
                    "A TV recusou a conexão sem exibir o pedido de permissão. \
                     Isso costuma acontecer quando a lista de dispositivos da TV está cheia ou com \
                     entradas bloqueadas. Na TV, abra Configurações → Geral → Gerenciador de \
                     Dispositivos Externos → Gerenciador de Conexão de Dispositivos → Lista de \
                     Dispositivos, remova dispositivos antigos e tente conectar de novo."
                        .to_string(),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sessão ativa
// ---------------------------------------------------------------------------

/// Trata mensagens recebidas da TV: encaminha o JSON bruto ao frontend e
/// resolve esperas específicas (ex.: lista de apps).
async fn handle_incoming(app: &AppHandle, txt: &str) {
    let _ = app.emit("tv://message", txt);

    let Ok(value) = serde_json::from_str::<serde_json::Value>(txt) else {
        return;
    };

    if value.get("event").and_then(|e| e.as_str()) == Some("ed.installedApp.get") {
        let apps = parse_apps(&value);
        if apps.is_empty() {
            return;
        }
        let state = app.state::<AppState>();
        let waiter = {
            let mut guard = state.apps_waiters.lock().await;
            guard.pop()
        };
        if let Some(tx) = waiter {
            let _ = tx.send(apps);
        }
    }
}

/// Loop de leitura/escrita da sessão. Termina quando o canal fecha ou quando
/// o remetente de comandos é descartado (desconexão pedida pela interface).
pub async fn run_session(
    app: AppHandle,
    ws: Ws,
    mut rx: mpsc::UnboundedReceiver<String>,
    session_id: u64,
    ip: String,
) {
    let (mut sink, mut stream) = ws.split();

    let reason = loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                None => break "A TV encerrou a conexão".to_string(),
                Some(Ok(Message::Text(txt))) => {
                    handle_incoming(&app, txt.as_str()).await;
                }
                Some(Ok(Message::Ping(payload))) => {
                    if sink.send(Message::Pong(payload)).await.is_err() {
                        break "Falha ao manter a conexão".to_string();
                    }
                }
                Some(Ok(Message::Close(_))) => break "A TV encerrou a conexão".to_string(),
                Some(Ok(_)) => {}
                Some(Err(e)) => break format!("Erro no canal de controle: {e}"),
            },
            outgoing = rx.recv() => match outgoing {
                None => break "Desconectado".to_string(),
                Some(text) => {
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        break "Falha ao enviar o comando".to_string();
                    }
                }
            },
        }
    };

    let _ = sink.close().await;

    let state = app.state::<AppState>();
    let mut guard = state.session.lock().await;
    let still_ours = guard.as_ref().map(|s| s.id) == Some(session_id);
    if still_ours {
        *guard = None;
        drop(guard);
        emit_status(&app, "disconnected", Some(&ip), Some(&reason), None);
    }
}
