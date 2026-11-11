//! Teste manual do lançamento de app pelo evento WebSocket (`ed.apps.launch`).
//!
//! Uso:
//!   cargo run --release --example launch-test -- <token> <appId> [ip] [nome]
//!
//! Exemplo:
//!   cargo run --release --example launch-test -- 11323558 3201606009684 192.168.0.101 tauri-remote

use std::time::Duration;

use futures_util::SinkExt;
use samsung_remote_lib::local::connect_socket;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let token = args.next().ok_or("informe o token (1º argumento)")?;
    let app_id = args.next().ok_or("informe o appId (2º argumento)")?;
    let host = args.next().unwrap_or_else(|| "192.168.0.101".to_string());
    let name = args.next().unwrap_or_else(|| "MeuApp".to_string());

    println!("conectando em {host}:8002 como \"{name}\"…");
    let (mut ws, _) = connect_socket(&host, &name, Some(&token), Duration::from_secs(8)).await?;
    println!("conectado!");

    let payload = json!({
        "method": "ms.channel.emit",
        "params": {
            "event": "ed.apps.launch",
            "to": "host",
            "data": { "action_type": "DEEP_LINK", "appId": app_id, "metaTag": "" }
        }
    })
    .to_string();

    ws.send(Message::Text(payload.into()))
        .await
        .map_err(|e| format!("falha ao enviar: {e}"))?;
    println!("evento ed.apps.launch enviado para {app_id}");

    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(())
}
