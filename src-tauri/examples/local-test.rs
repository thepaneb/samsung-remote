//! Teste manual da conexão local (porta 8002) usando o mesmo código do app.
//!
//! Uso:
//!   cargo run --release --example local-test -- <token> [ip] [nome]
//!
//! Exemplo:
//!   cargo run --release --example local-test -- 12112214 192.168.0.101 MeuApp
//!
//! O que ele faz: conecta, pede a lista de apps instalados e imprime os nomes.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use samsung_remote_lib::local::connect_socket;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let token = args.next().ok_or("informe o token (1º argumento)")?;
    let host = args.next().unwrap_or_else(|| "192.168.0.101".to_string());
    let name = args.next().unwrap_or_else(|| "MeuApp".to_string());

    println!("conectando em {host}:8002 como \"{name}\"…");
    let (mut ws, returned_token) =
        connect_socket(&host, &name, Some(&token), Duration::from_secs(8)).await?;
    println!("conectado! token devolvido: {returned_token:?}");

    let request = json!({
        "method": "ms.channel.emit",
        "params": { "event": "ed.installedApp.get", "to": "host" }
    })
    .to_string();

    ws.send(Message::Text(request.into()))
        .await
        .map_err(|e| format!("falha ao enviar: {e}"))?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("a TV não respondeu com a lista".into());
        }

        let frame = match tokio::time::timeout(remaining, ws.next()).await {
            Err(_) => return Err("tempo esgotado".into()),
            Ok(None) => return Err("conexão encerrada pela TV".into()),
            Ok(Some(Err(e))) => return Err(format!("erro no canal: {e}")),
            Ok(Some(Ok(frame))) => frame,
        };

        let text = match frame {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
            _ => continue,
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if value.get("event").and_then(Value::as_str) == Some("ed.installedApp.get") {
            let list = value
                .get("data")
                .and_then(|d| d.get("data"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            println!("{} apps instalados:", list.len());
            for item in list {
                println!(
                    "  {} | {}",
                    item.get("appId").and_then(Value::as_str).unwrap_or("?"),
                    item.get("name").and_then(Value::as_str).unwrap_or("?")
                );
            }
            break;
        }
    }

    Ok(())
}
