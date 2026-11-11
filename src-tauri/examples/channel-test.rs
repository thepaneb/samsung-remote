//! Teste manual do teclado numérico local (KEY_2 0 0 0 + OK) — o mesmo caminho
//! do atalho TV Plus.
//!
//! Uso:
//!   cargo run --release --example channel-test -- <token> [canal] [ip] [nome]
//!
//! Exemplo:
//!   cargo run --release --example channel-test -- 11323558 2000 192.168.0.101 tauri-remote

use std::time::Duration;

use futures_util::SinkExt;
use samsung_remote_lib::local::connect_socket;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

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

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let token = args.next().ok_or("informe o token (1º argumento)")?;
    let channel = args.next().unwrap_or_else(|| "2000".to_string());
    let host = args.next().unwrap_or_else(|| "192.168.0.101".to_string());
    let name = args.next().unwrap_or_else(|| "MeuApp".to_string());

    println!("conectando em {host}:8002 como \"{name}\"…");
    let (mut ws, returned) =
        connect_socket(&host, &name, Some(&token), Duration::from_secs(8)).await?;
    println!("conectado! token devolvido: {returned:?}");

    for ch in channel.chars().filter(char::is_ascii_digit) {
        let key = format!("KEY_{ch}");
        ws.send(Message::Text(key_payload(&key).into()))
            .await
            .map_err(|e| format!("falha ao enviar {key}: {e}"))?;
        println!("enviado {key}");
        tokio::time::sleep(Duration::from_millis(260)).await;
    }

    ws.send(Message::Text(key_payload("KEY_ENTER").into()))
        .await
        .map_err(|e| format!("falha ao enviar KEY_ENTER: {e}"))?;
    println!("enviado KEY_ENTER");

    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}
