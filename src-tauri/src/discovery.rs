//! Descoberta de TVs Samsung na rede local (SSDP / M-SEARCH em UDP multicast).
//!
//! Fluxo: enviamos um M-SEARCH para `239.255.255.250:1900` pedindo o alvo de
//! TVs Samsung (`urn:samsung.com:device:RemoteControlReceiver:1`) e, como
//! reserva, `ssdp:all`. Cada resposta traz o IP da TV no cabeçalho `LOCATION`.
//! Em seguida consultamos a API HTTP local (`http://<ip>:8000/api/v2/`) para
//! obter o nome e o modelo — assim a lista mostra "TV da Sala" com modelo, em
//! vez de só o IP.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use serde::Serialize;
use serde_json::Value;
use tokio::net::UdpSocket;

const SSDP_ADDR: &str = "239.255.255.250:1900";
/// Alvo anunciado pelas TVs Samsung no canal de controle remoto.
const ST_SAMSUNG: &str = "urn:samsung.com:device:RemoteControlReceiver:1";
/// Busca ampla, usada como reserva para modelos que não respondem ao alvo acima.
const ST_ALL: &str = "ssdp:all";
/// Limite de dispositivos sondados por HTTP (evita vasculhar a rede inteira).
const MAX_PROBES: usize = 24;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredTv {
    pub host: String,
    pub name: Option<String>,
    pub model: Option<String>,
    /// já é a TV pareada neste aparelho
    pub paired: bool,
}

fn search_message(st: &str) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: 2\r\n\
         ST: {st}\r\n\r\n"
    )
}

/// Extrai o IP a partir do `LOCATION` de uma resposta SSDP.
///
/// O segundo item deixa claro se a resposta "parece" de TV Samsung (portas
/// 8000/8001/8002/9197 no `LOCATION` ou "samsung" em algum cabeçalho): respostas
/// de outros aparelhos da rede (roteador, impressora…) só entram na lista final
/// se a sondagem HTTP confirmar que há uma API de TV naquele IP.
fn extract_ip(response: &str) -> Option<(String, bool)> {
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
    let is_tv_port = [":8000/", ":8001/", ":8002/", ":9197/"]
        .iter()
        .any(|port| loc.contains(port));
    let samsung_like = is_tv_port || haystack.contains("samsung");

    let after_scheme = loc.split("//").nth(1)?;
    let host = after_scheme.split([':', '/']).next()?.trim();
    if host.is_empty() {
        None
    } else {
        Some((host.to_string(), samsung_like))
    }
}

/// Dispara o M-SEARCH e coleta os IPs que responderem dentro do tempo dado.
///
/// O alvo Samsung vai no primeiro envio; no decorrer da espera reforçamos com
/// `ssdp:all` e com um segundo envio do alvo Samsung (alguns modelos perdem o
/// primeiro pacote).
async fn discover_ips(timeout: Duration) -> Vec<(String, bool)> {
    let mut found: Vec<(String, bool)> = Vec::new();

    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
        return found;
    };
    let _ = socket.set_broadcast(true);
    let _ = socket.set_multicast_ttl_v4(2);

    let Ok(target) = SSDP_ADDR.parse::<SocketAddr>() else {
        return found;
    };

    let _ = socket
        .send_to(search_message(ST_SAMSUNG).as_bytes(), target)
        .await;

    let started = Instant::now();
    let mut resent_all = false;
    let mut resent_samsung = false;
    let mut buf = vec![0u8; 8192];

    loop {
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            break;
        }

        if !resent_all && elapsed > timeout / 3 {
            let _ = socket.send_to(search_message(ST_ALL).as_bytes(), target).await;
            resent_all = true;
        }
        if !resent_samsung && elapsed > timeout * 2 / 3 {
            let _ = socket
                .send_to(search_message(ST_SAMSUNG).as_bytes(), target)
                .await;
            resent_samsung = true;
        }

        match tokio::time::timeout(Duration::from_millis(250), socket.recv_from(&mut buf)).await {
            Ok(Ok((n, _src))) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some((ip, samsung_like)) = extract_ip(&text) {
                    if !found.iter().any(|(known, _)| known == &ip) {
                        found.push((ip, samsung_like));
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => continue, // janela sem resposta: segue esperando
        }
    }

    found
}

struct Info {
    name: Option<String>,
    model: Option<String>,
}

fn str_at(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Nome e modelo da TV pela API HTTP local (endpoint que não pede autorização).
async fn fetch_info(ip: &str) -> Option<Info> {
    let client = reqwest::Client::builder()
        // a TV usa certificado autoassinado na porta 8002
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(2500))
        .build()
        .ok()?;

    for url in [
        format!("http://{ip}:8000/api/v2/"),
        format!("https://{ip}:8002/api/v2/"),
    ] {
        let Ok(response) = client.get(&url).send().await else {
            continue;
        };
        let Ok(value) = response.json::<Value>().await else {
            continue;
        };

        let device = value.get("device").unwrap_or(&value);
        let name = str_at(device, "name");
        let model = str_at(device, "modelName").or_else(|| str_at(device, "model"));
        if name.is_some() || model.is_some() {
            return Some(Info { name, model });
        }
    }

    None
}

/// Descobre as TVs Samsung da rede: SSDP para achar os IPs e a API local para
/// enriquecer com nome/modelo. A lista sai ordenada por nome (ou IP).
pub async fn discover(timeout: Duration) -> Vec<DiscoveredTv> {
    let candidates = discover_ips(timeout).await;

    // Sonda os candidatos em paralelo (com teto, para não varrer a rede toda).
    let probes = candidates
        .iter()
        .take(MAX_PROBES)
        .map(|(ip, samsung_like)| {
            let ip = ip.clone();
            let samsung_like = *samsung_like;
            async move {
                let info = fetch_info(&ip).await;
                (ip, samsung_like, info)
            }
        });
    let results = join_all(probes).await;

    let mut seen = HashSet::new();
    let mut list: Vec<DiscoveredTv> = Vec::new();
    for (host, samsung_like, info) in results {
        if !seen.insert(host.clone()) {
            continue;
        }

        let (name, model) = match info {
            Some(info) => (info.name, info.model),
            // Sem resposta HTTP: mantém se a resposta SSDP parecia de TV Samsung
            // (pode estar com a porta 8000 bloqueada, mas o pareamento vale a pena).
            None if samsung_like => (None, None),
            None => continue,
        };

        list.push(DiscoveredTv {
            host,
            name,
            model,
            paired: false,
        });
    }

    list.sort_by(|a, b| {
        let an = a.name.as_deref().unwrap_or(&a.host).to_lowercase();
        let bn = b.name.as_deref().unwrap_or(&b.host).to_lowercase();
        an.cmp(&bn).then_with(|| a.host.cmp(&b.host))
    });
    list
}
