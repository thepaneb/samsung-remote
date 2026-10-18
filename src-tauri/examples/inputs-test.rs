//! Teste local da leitura de entradas usando exatamente o mesmo código do app.
//!
//! Uso: cargo run --release --example inputs-test

use samsung_remote_lib::smartthings::{self, StConfig};

fn main() {
    let appdata = std::env::var("APPDATA").expect("APPDATA não definido");
    let path = format!(r"{appdata}\com.samsungremote.desktop\smartthings.json");
    let text = std::fs::read_to_string(&path).expect("não encontrei smartthings.json");
    let cfg: StConfig = serde_json::from_str(&text).expect("smartthings.json inválido");

    println!(
        "token: {}",
        cfg.token
            .as_deref()
            .map(|t| &t[..8.min(t.len())])
            .unwrap_or("-")
    );
    println!("device: {:?}", cfg.device_id);
    println!("componentes: {:?}", cfg.components);
    println!(
        "componente p/ samsungvd.mediaInputSource: {:?}",
        cfg.cap_components.get("samsungvd.mediaInputSource")
    );
    println!(
        "tem samsungvd.mediaInputSource? {}",
        cfg.capabilities.iter().any(|c| c == "samsungvd.mediaInputSource")
    );
    println!(
        "tem mediaInputSource? {}",
        cfg.capabilities.iter().any(|c| c == "mediaInputSource")
    );

    let rt = tokio::runtime::Runtime::new().expect("falha ao criar runtime");

    // Diagnóstico bruto: mesma URL que o app usa, resposta crua.
    rt.block_on(async {
        let (token, device) = match (&cfg.token, &cfg.device_id) {
            (Some(t), Some(d)) => (t.clone(), d.clone()),
            _ => return,
        };
        let url = format!(
            "https://api.smartthings.com/v1/devices/{device}/components/main/capabilities/samsungvd.mediaInputSource/status"
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("cliente");
        match client.get(&url).bearer_auth(&token).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                println!("--- diagnóstico bruto ---");
                println!("URL: {url}");
                println!("status: {status}");
                println!(
                    "body: {}",
                    if body.len() > 600 { &body[..600] } else { &body }
                );
            }
            Err(err) => println!("--- diagnóstico bruto ---\nERRO DE REDE: {err}"),
        }
    });

    match rt.block_on(smartthings::list_inputs_core(&cfg)) {
        Ok(inputs) => {
            println!("OK: {} entradas", inputs.len());
            for input in &inputs {
                println!(
                    "  - {} ({}){}",
                    input.name,
                    input.id,
                    if input.current { " [atual]" } else { "" }
                );
            }
        }
        Err(err) => println!("ERRO: {err}"),
    }
}
