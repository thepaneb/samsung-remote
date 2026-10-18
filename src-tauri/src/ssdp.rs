//! Descoberta de TVs Samsung via SSDP (M-SEARCH em multicast UDP).

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const SSDP_ADDR: &str = "239.255.255.250:1900";

fn search_message(st: &str) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {st}\r\n\r\n"
    )
}

/// Extrai o IP do host a partir de uma resposta SSDP, aceitando apenas
/// respostas que pareçam de TVs Samsung (portas 8000/8001/8002/9197 ou
/// menção a "samsung" nos headers).
fn extract_ip(response: &str) -> Option<String> {
    let mut location: Option<String> = None;
    let mut haystack = String::new();

    for line in response.lines() {
        let lower = line.to_lowercase();
        if let Some(value) = lower.strip_prefix("location:") {
            location = Some(value.trim().to_string());
        }
        haystack.push_str(&lower);
        haystack.push('\n');
    }

    let loc = location?;
    let is_tv_port = loc.contains(":8000/")
        || loc.contains(":8001/")
        || loc.contains(":8002/")
        || loc.contains(":9197/");
    let mentions_samsung = haystack.contains("samsung");
    if !is_tv_port && !mentions_samsung {
        return None;
    }

    let after_scheme = loc.split("//").nth(1)?;
    let host = after_scheme.split([':', '/']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Envia M-SEARCH para o alvo específico de TVs Samsung e, depois,
/// `ssdp:all` como redundância. Retorna os IPs encontrados (sem repetição).
pub async fn discover_ips(timeout: Duration) -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return ips,
    };
    let _ = socket.set_broadcast(true);

    let target: SocketAddr = match SSDP_ADDR.parse() {
        Ok(a) => a,
        Err(_) => return ips,
    };

    let _ = socket
        .send_to(
            search_message("urn:samsung.com:device:RemoteControlReceiver:1").as_bytes(),
            target,
        )
        .await;

    let started = Instant::now();
    let mut sent_second = false;
    let mut buf = vec![0u8; 8192];

    loop {
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            break;
        }

        if !sent_second && elapsed > timeout / 3 {
            let _ = socket.send_to(search_message("ssdp:all").as_bytes(), target).await;
            sent_second = true;
        }

        match tokio::time::timeout(Duration::from_millis(250), socket.recv_from(&mut buf)).await {
            Ok(Ok((n, _src))) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some(ip) = extract_ip(&text) {
                    if !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }

    ips
}
