//! Teste de integração manual contra uma TV Samsung real.
//!
//! Uso: `cargo run --release --example pair-test -- 192.168.0.101`
//!
//! Conecta no canal de controle, aguarda a autorização (popup na TV),
//! envia Volume+/Volume-, pede a lista de apps e imprime o resultado.

use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

fn key_message(key: &str) -> String {
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

fn apps_request() -> String {
    serde_json::json!({
        "method": "ms.channel.emit",
        "params": { "event": "ed.installedApp.get", "to": "host" }
    })
    .to_string()
}

#[tokio::main]
async fn main() {
    let ip = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "192.168.0.101".to_string());
    let client_name = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "Controle Tauri".to_string());
    let name = base64::engine::general_purpose::STANDARD.encode(&client_name);
    let url = format!("ws://{ip}:8001/api/v2/channels/samsung.remote.control?name={name}");
    println!("Conectando em {url} (nome: {client_name})");

    let (mut ws, _) = match tokio_tungstenite::connect_async(&url).await {
        Ok(pair) => pair,
        Err(err) => {
            println!("FALHA ao conectar: {err}");
            return;
        }
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let mut authorized = false;

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        match tokio::time::timeout(remaining.min(Duration::from_secs(2)), ws.next()).await {
            Err(_) => continue,
            Ok(None) => {
                println!("Conexão encerrada pela TV.");
                return;
            }
            Ok(Some(Err(err))) => {
                println!("Erro no canal: {err}");
                return;
            }
            Ok(Some(Ok(Message::Text(txt)))) => {
                println!("<< {txt}");
                let value: serde_json::Value =
                    serde_json::from_str(txt.as_str()).unwrap_or_default();
                match value.get("event").and_then(|e| e.as_str()).unwrap_or("") {
                    "ms.channel.connect" => {
                        let token = value
                            .get("data")
                            .and_then(|d| d.get("token"))
                            .and_then(|t| t.as_str())
                            .map(str::to_string);
                        println!("AUTORIZADO. token = {token:?}");
                        authorized = true;
                        break;
                    }
                    "ms.channel.unauthorized" => {
                        println!("RECUSADO pela TV (ms.channel.unauthorized).");
                        return;
                    }
                    _ => {}
                }
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                let _ = ws.send(Message::Pong(payload)).await;
            }
            Ok(Some(Ok(_))) => {}
        }
    }

    if !authorized {
        println!("TEMPO ESGOTADO: nenhuma autorização recebida.");
        return;
    }

    println!(">> enviando KEY_VOLUP");
    if let Err(err) = ws.send(Message::Text(key_message("KEY_VOLUP").into())).await {
        println!("FALHA ao enviar tecla: {err}");
        return;
    }
    tokio::time::sleep(Duration::from_millis(800)).await;

    println!(">> enviando KEY_VOLDOWN (restaura o volume)");
    let _ = ws
        .send(Message::Text(key_message("KEY_VOLDOWN").into()))
        .await;

    println!(">> pedindo lista de apps (ed.installedApp.get)");
    if let Err(err) = ws.send(Message::Text(apps_request().into())).await {
        println!("FALHA ao pedir apps: {err}");
        return;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(txt)))) => {
                let value: serde_json::Value =
                    serde_json::from_str(txt.as_str()).unwrap_or_default();
                if value.get("event").and_then(|e| e.as_str()) == Some("ed.installedApp.get") {
                    let apps = value
                        .get("data")
                        .and_then(|d| d.get("data"))
                        .and_then(|a| a.as_array())
                        .cloned()
                        .unwrap_or_default();
                    println!("APPS: {} encontrados", apps.len());
                    if let Some(first) = apps.first() {
                        println!("Primeiro item bruto: {first}");
                    }
                    for app in apps.iter().take(10) {                        let name = app
                            .get("name")
                            .or_else(|| app.get("appName"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let id = app.get("appId").and_then(|v| v.as_str()).unwrap_or("?");
                        let app_type = app.get("app_type").and_then(|v| v.as_i64());
                        println!("  - {name} | {id} | app_type={app_type:?}");
                    }
                    break;
                }
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                let _ = ws.send(Message::Pong(payload)).await;
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(err))) => {
                println!("Erro no canal: {err}");
                break;
            }
            Ok(None) => break,
            Err(_) => {
                println!("Sem resposta para ed.installedApp.get.");
                break;
            }
        }
    }

    let _ = ws.close(None).await;
    println!("FIM");
}
