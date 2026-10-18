//! Leitura de informações da TV pela API HTTP local da Samsung
//! (`http://ip:8000/api/v2/`, com fallback `https://ip:8002`).

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TvDevice {
    pub ip: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub power_state: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub paired: bool,
    #[serde(default)]
    pub reachable: bool,
}

pub fn http_client(timeout_secs: u64) -> Option<reqwest::Client> {
    reqwest::Client::builder()
        // TVs usam certificado autoassinado na porta 8002
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .ok()
}

fn str_at(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse(ip: &str, v: &serde_json::Value) -> Option<TvDevice> {
    let d = v.get("device").unwrap_or(v);
    let name = str_at(d, "name");
    let model_name = str_at(d, "modelName");
    let model = str_at(d, "model");
    if name.is_none() && model_name.is_none() {
        return None;
    }
    Some(TvDevice {
        ip: ip.to_string(),
        name,
        model,
        model_name,
        mac: str_at(d, "wifiMac").or_else(|| str_at(d, "mac")),
        power_state: str_at(d, "PowerState"),
        token: str_at(d, "token"),
        paired: false,
        reachable: true,
    })
}

pub async fn fetch_device_info(ip: &str) -> Option<TvDevice> {
    let client = http_client(4)?;
    for url in [
        format!("http://{ip}:8001/api/v2/"),
        format!("http://{ip}:8000/api/v2/"),
        format!("https://{ip}:8002/api/v2/"),
    ] {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(device) = parse(ip, &v) {
                    return Some(device);
                }
            }
        }
    }
    None
}
