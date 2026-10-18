//! Wake-on-LAN: monta e envia o "magic packet" para ligar a TV.

use tokio::net::UdpSocket;

fn magic_packet(mac: &str) -> Result<Vec<u8>, String> {
    let octets: Vec<u8> = mac
        .split([':', '-'])
        .map(|part| u8::from_str_radix(part.trim(), 16))
        .collect::<Result<_, _>>()
        .map_err(|_| "Endereço MAC inválido".to_string())?;

    if octets.len() != 6 {
        return Err("Endereço MAC inválido".to_string());
    }

    let mut packet = Vec::with_capacity(102);
    packet.extend_from_slice(&[0xFF; 6]);
    for _ in 0..16 {
        packet.extend_from_slice(&octets);
    }
    Ok(packet)
}

pub async fn wake(mac: &str) -> Result<(), String> {
    let packet = magic_packet(mac)?;

    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("Falha ao abrir socket UDP: {e}"))?;
    let _ = socket.set_broadcast(true);

    let mut sent = false;
    for target in ["255.255.255.255:9", "255.255.255.255:7", "224.0.0.1:9"] {
        if socket.send_to(&packet, target).await.is_ok() {
            sent = true;
        }
    }

    if sent {
        Ok(())
    } else {
        Err("Não foi possível enviar o pacote Wake-on-LAN".to_string())
    }
}
