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
use serde::Serialize;
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

pub fn launch_message(app_id: &str) -> String {
    serde_json::json!({
        "method": "ms.channel.emit",
        "params": {
            "event": "ed.apps.launch",
            "to": "host",
            "data": {
                "appId": app_id,
                "action_type": "DEEP_LINK",
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
                    "A TV recusou a conexão. Tente novamente e aceite o pedido de permissão na tela."
                        .to_string(),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sessão ativa
// ---------------------------------------------------------------------------

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
                    let _ = app.emit("tv://message", txt.as_str());
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
