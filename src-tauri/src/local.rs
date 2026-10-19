//! Controle local da TV pela rede (WebSocket seguro na porta 8002).
//!
//! Vantagens sobre a nuvem: responde na hora, não depende da internet da
//! Samsung, permite a tecla GUIA (guia de canais) e devolve a **lista real de
//! apps instalados** na TV.
//!
//! Pareamento: na primeira vez conectamos sem token; a TV mostra um pop-up e,
//! ao aceitar com o controle, envia um token que fica guardado aqui. Se o
//! token for recusado depois, é só parear de novo.
//!
//! Protocolo (implementado também pela biblioteca `samsungtvws`):
//! - URL: `wss://<ip>:8002/api/v2/channels/samsung.remote.control?name=<base64>&token=<token>`
//! - Teclas: `{"method":"ms.remote.control","params":{"Cmd":"Click","DataOfCmd":"KEY_*",...}}`
//! - Apps instalados: `{"method":"ms.channel.emit","params":{"event":"ed.installedApp.get","to":"host"}}`
//! - Abrir app: `ed.apps.launch` com `appId`; digitar texto: `SendInputString`.
//! - O certificado TLS da TV é autoassinado: aceitamos (é a própria TV na LAN).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async, WebSocketStream};
use tokio_rustls::rustls;
use tokio_rustls::TlsConnector;

const PORT: u16 = 8002;
const ENDPOINT: &str = "samsung.remote.control";
const DEFAULT_NAME: &str = "Samsung Remote";
/// Espera padrão para conectar/responder (teclas).
const WAIT_CONNECT: Duration = Duration::from_secs(3);
/// Espera do pareamento: tempo para aceitar o pop-up na TV.
const WAIT_PAIR: Duration = Duration::from_secs(90);

// ---------------------------------------------------------------------------
// Configuração persistida (local.json)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalConfig {
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    /// usar a conexão direta como transporte preferido
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatus {
    pub host: Option<String>,
    pub name: Option<String>,
    pub has_token: bool,
    pub connected: bool,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalApp {
    pub app_id: String,
    pub name: String,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("local.json"))
}

pub fn load(app: &AppHandle) -> LocalConfig {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, cfg: &LocalConfig) {
    if let Some(p) = path(app) {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(p, json);
        }
    }
}

/// Lista de apps já carregada da TV (para abrir o diálogo sem esperar).
fn cache_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("local-apps.json"))
}

fn save_cache(app: &AppHandle, apps: &[LocalApp]) {
    if let Some(p) = cache_path(app) {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(apps) {
            let _ = std::fs::write(p, json);
        }
    }
}

pub fn cached_apps(app: &AppHandle) -> Vec<LocalApp> {
    cache_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Estado (socket vivo entre comandos)
// ---------------------------------------------------------------------------

pub struct LocalState {
    inner: Mutex<Inner>,
}

struct Inner {
    ws: Option<Ws>,
}

type Ws = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;

impl LocalState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner { ws: None }),
        }
    }
}

impl Default for LocalState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TLS (aceita o certificado autoassinado da TV)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme as S;
        vec![
            S::RSA_PKCS1_SHA256,
            S::RSA_PKCS1_SHA384,
            S::RSA_PKCS1_SHA512,
            S::ECDSA_NISTP256_SHA256,
            S::ECDSA_NISTP384_SHA384,
            S::RSA_PSS_SHA256,
            S::RSA_PSS_SHA384,
            S::RSA_PSS_SHA512,
            S::ED25519,
        ]
    }
}

// ---------------------------------------------------------------------------
// Utilitários
// ---------------------------------------------------------------------------

/// Base64 padrão (evita dependência extra; usado no nome e no texto digitado).
fn b64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Base64 codificado para uso na query string (como o yarl do samsungtvws).
fn b64_query(input: &str) -> String {
    b64(input.as_bytes())
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}

fn key_payload(key: &str) -> String {
    json!({
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

// ---------------------------------------------------------------------------
// Conexão
// ---------------------------------------------------------------------------

/// Abre o WebSocket com a TV e aguarda o evento de conexão.
///
/// Devolve o socket e, quando a TV manda, o token (novo ou o mesmo).
/// Público também para o exemplo de diagnóstico `examples/local-test.rs`.
pub async fn connect_socket(
    host: &str,
    name: &str,
    token: Option<&str>,
    wait: Duration,
) -> Result<(Ws, Option<String>), String> {
    let tls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(tls_config));

    let addr = format!("{host}:{PORT}");
    let stream = tokio::time::timeout(WAIT_CONNECT, TcpStream::connect(&addr))
        .await
        .map_err(|_| format!("Tempo esgotado ao conectar em {addr}. A TV está ligada?"))?
        .map_err(|e| format!("Não foi possível conectar em {addr}: {e}"))?;

    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| format!("Endereço inválido: {host}"))?;
    let tls = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| format!("Falha na conexão segura com a TV: {e}"))?;

    let mut query = format!("name={}", b64_query(name));
    if let Some(t) = token {
        query.push_str("&token=");
        query.push_str(t);
    }
    let url = format!("wss://{host}:{PORT}/api/v2/channels/{ENDPOINT}?{query}");

    let (mut ws, _) = client_async(url, tls)
        .await
        .map_err(|e| format!("Falha ao abrir o canal com a TV: {e}"))?;

    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(
                "Tempo esgotado. Se apareceu um pop-up na TV, aceite com o controle (Permitir) e tente de novo."
                    .into(),
            );
        }

        let result = next_text(&mut ws, remaining).await;
        let text = match result {
            Err(e) => return Err(e),
            Ok(text) => text,
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match value.get("event").and_then(Value::as_str).unwrap_or("") {
            "ms.channel.connect" => {
                let new_token = value
                    .get("data")
                    .and_then(|d| d.get("token"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Ok((ws, new_token));
            }
            "ms.channel.unauthorized" => {
                return Err(
                    "A TV não autorizou a conexão. Aceite o pop-up na TV com o controle e pareie de novo."
                        .into(),
                );
            }
            _ => {}
        }
    }
}

/// Lê a próxima mensagem de texto (frames de controle são ignorados).
async fn next_text(ws: &mut Ws, budget: Duration) -> Result<String, String> {
    let frame = match tokio::time::timeout(budget, ws.next()).await {
        Err(_) => return Err("A TV não respondeu a tempo.".into()),
        Ok(None) => return Err("A TV encerrou a conexão.".into()),
        Ok(Some(Err(e))) => return Err(format!("Erro no canal com a TV: {e}")),
        Ok(Some(Ok(frame))) => frame,
    };

    match frame {
        Message::Text(t) => Ok(t.to_string()),
        Message::Binary(b) => Ok(String::from_utf8_lossy(&b).to_string()),
        Message::Close(_) => Err("A TV encerrou a conexão.".into()),
        _ => Ok(String::new()),
    }
}

async fn ensure_ws(inner: &mut Inner, cfg: &LocalConfig, wait: Duration) -> Result<(), String> {
    if inner.ws.is_some() {
        return Ok(());
    }
    let host = cfg
        .host
        .clone()
        .ok_or("Informe o IP da TV na conexão direta.")?;
    let name = cfg.name.clone().unwrap_or_else(|| DEFAULT_NAME.to_string());
    let token = cfg
        .token
        .clone()
        .ok_or("Faça o pareamento com a TV para usar a conexão direta.")?;
    let (ws, _) = connect_socket(&host, &name, Some(&token), wait).await?;
    inner.ws = Some(ws);
    Ok(())
}

// ---------------------------------------------------------------------------
// Comandos (usados por commands.rs)
// ---------------------------------------------------------------------------

pub async fn status(app: &AppHandle, state: &LocalState) -> LocalStatus {
    let cfg = load(app);
    let inner = state.inner.lock().await;
    LocalStatus {
        host: cfg.host,
        name: cfg.name,
        has_token: cfg.token.is_some(),
        enabled: cfg.enabled,
        connected: inner.ws.is_some(),
    }
}

pub async fn pair(
    app: &AppHandle,
    state: &LocalState,
    host: String,
    name: String,
) -> Result<LocalStatus, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("Informe o IP da TV (ex.: 192.168.0.101).".into());
    }
    let name = {
        let n = name.trim().to_string();
        if n.is_empty() {
            DEFAULT_NAME.to_string()
        } else {
            n
        }
    };

    {
        let mut inner = state.inner.lock().await;
        inner.ws = None;
    }

    let (ws, token) = connect_socket(&host, &name, None, WAIT_PAIR).await?;
    let token = token.ok_or("A TV conectou, mas não enviou o token. Tente parear de novo.")?;

    let mut cfg = load(app);
    cfg.host = Some(host);
    cfg.name = Some(name);
    cfg.token = Some(token);
    cfg.enabled = true;
    save(app, &cfg);

    {
        let mut inner = state.inner.lock().await;
        inner.ws = Some(ws);
    }

    Ok(status(app, state).await)
}

pub async fn set_enabled(
    app: &AppHandle,
    state: &LocalState,
    enabled: bool,
) -> Result<LocalStatus, String> {
    let mut cfg = load(app);
    cfg.enabled = enabled;
    save(app, &cfg);

    if !enabled {
        let mut inner = state.inner.lock().await;
        inner.ws = None;
    }

    Ok(status(app, state).await)
}

pub async fn forget(app: &AppHandle, state: &LocalState) -> Result<LocalStatus, String> {
    let mut cfg = load(app);
    cfg.token = None;
    cfg.enabled = false;
    save(app, &cfg);

    {
        let mut inner = state.inner.lock().await;
        inner.ws = None;
    }

    Ok(status(app, state).await)
}

async fn send_json(
    app: &AppHandle,
    state: &LocalState,
    payload: String,
) -> Result<(), String> {
    let cfg = load(app);
    let mut inner = state.inner.lock().await;

    ensure_ws(&mut inner, &cfg, WAIT_CONNECT).await?;

    // Primeira tentativa; se o socket estiver morto, reconecta e tenta de novo.
    let first = {
        let ws = inner.ws.as_mut().expect("socket conectado");
        ws.send(Message::Text(payload.clone().into())).await
    };

    match first {
        Ok(()) => Ok(()),
        Err(_) => {
            inner.ws = None;
            ensure_ws(&mut inner, &cfg, WAIT_CONNECT).await?;
            let ws = inner.ws.as_mut().expect("socket reconectado");
            ws.send(Message::Text(payload.into()))
                .await
                .map_err(|e| format!("Falha ao enviar o comando para a TV: {e}"))
        }
    }
}

/// Envia uma tecla do controle (KEY_*).
pub async fn send_key(app: &AppHandle, state: &LocalState, key: &str) -> Result<(), String> {
    send_json(app, state, key_payload(key)).await
}

/// Sintoniza um canal pelo teclado numérico (dígitos + OK).
pub async fn set_channel(
    app: &AppHandle,
    state: &LocalState,
    value: &str,
) -> Result<(), String> {
    if value.contains('.') {
        return Err("Canal com ponto decimal: use o modo nuvem.".into());
    }
    let digits: Vec<char> = value.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err("Número de canal inválido.".into());
    }

    for digit in digits {
        send_key(app, state, &format!("KEY_{digit}")).await?;
        tokio::time::sleep(Duration::from_millis(260)).await;
    }
    send_key(app, state, "KEY_ENTER").await
}

/// Abre um app instalado pelo ID.
pub async fn launch(app: &AppHandle, state: &LocalState, app_id: &str) -> Result<(), String> {
    send_json(
        app,
        state,
        json!({
            "method": "ms.channel.emit",
            "params": {
                "event": "ed.apps.launch",
                "to": "host",
                "data": {
                    "action_type": "DEEP_LINK",
                    "appId": app_id,
                    "metaTag": ""
                }
            }
        })
        .to_string(),
    )
    .await
}

/// Digita um texto na TV (vai para o campo em foco, como o teclado da TV).
pub async fn send_text(app: &AppHandle, state: &LocalState, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }

    // Algumas TVs exigem este aviso antes de aceitar texto.
    send_json(
        app,
        state,
        json!({
            "method": "ms.channel.emit",
            "params": { "event": "custom.remote.textReceived", "to": "broadcast" }
        })
        .to_string(),
    )
    .await?;

    send_json(
        app,
        state,
        json!({
            "method": "ms.remote.control",
            "params": {
                "Cmd": b64(text.as_bytes()),
                "DataOfCmd": "base64",
                "TypeOfRemote": "SendInputString"
            }
        })
        .to_string(),
    )
    .await?;

    send_json(
        app,
        state,
        json!({
            "method": "ms.remote.control",
            "params": { "TypeOfRemote": "SendInputEnd" }
        })
        .to_string(),
    )
    .await
}

/// Pede a lista de apps instalados à TV (evento `ed.installedApp.get`).
pub async fn apps(app: &AppHandle, state: &LocalState) -> Result<Vec<LocalApp>, String> {
    let cfg = load(app);
    let mut inner = state.inner.lock().await;
    ensure_ws(&mut inner, &cfg, WAIT_CONNECT).await?;

    let payload = json!({
        "method": "ms.channel.emit",
        "params": { "event": "ed.installedApp.get", "to": "host" }
    })
    .to_string();

    {
        let ws = inner.ws.as_mut().expect("socket conectado");
        ws.send(Message::Text(payload.into()))
            .await
            .map_err(|e| format!("Falha ao pedir a lista de apps: {e}"))?;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("A TV não devolveu a lista de apps.".into());
        }

        let result = {
            let ws = inner.ws.as_mut().expect("socket conectado");
            next_text(ws, remaining).await
        };
        let text = match result {
            Err(e) => {
                inner.ws = None;
                return Err(e);
            }
            Ok(text) => text,
        };

        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match value.get("event").and_then(Value::as_str).unwrap_or("") {
            "ed.installedApp.get" => {
                let list = value
                    .get("data")
                    .and_then(|d| d.get("data"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                let mut apps: Vec<LocalApp> = list
                    .iter()
                    .filter_map(|item| {
                        let app_id = item
                            .get("appId")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if app_id.is_empty() {
                            return None;
                        }
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        Some(LocalApp {
                            app_id,
                            name: if name.is_empty() {
                                "App".to_string()
                            } else {
                                name
                            },
                        })
                    })
                    .collect();

                let mut seen = std::collections::HashSet::new();
                apps.retain(|a| seen.insert(a.app_id.clone()));
                apps.sort_by_key(|a| a.name.to_lowercase());
                save_cache(app, &apps);
                return Ok(apps);
            }
            "ms.channel.unauthorized" => {
                inner.ws = None;
                return Err("A TV não autorizou a conexão. Pareie de novo.".into());
            }
            _ => {}
        }
    }
}
