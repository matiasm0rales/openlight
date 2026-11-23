//! Broadcasts UDP de discovery y respuesta a `REQ_DEVINFO`.
//!
//! Como los focos reales (`tinytuya/scanner.py`, `core/udp_helper.py`):
//! - 3.1 anuncia en el puerto 6666 en JSON plano.
//! - 3.3 y 3.4 anuncian en el 6667, cifrado con `md5("yGAdlopoPVldABfn")`.
//! - 3.5 anuncia en el 7000 con frames `0x6699` (GCM con la misma clave), como
//!   `fixtures/proto/udp.json`, y contesta los `REQ_DEVINFO` que la app manda al 7000, a la IP
//!   que dice el pedido.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use openlight_types::ProtocolVersion;
use serde_json::{Value, json};
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

use crate::BroadcastConfig;
use crate::server::Shared;
use crate::wire::cmd;

/// Puerto de anuncios de la versión.
pub fn default_port(version: ProtocolVersion) -> u16 {
    match version {
        ProtocolVersion::V31 => 6666,
        ProtocolVersion::V33 | ProtocolVersion::V34 => 6667,
        ProtocolVersion::V35 => 7000,
    }
}

/// JSON que anuncia el foco.
pub(crate) fn announcement(shared: &Shared, ip: IpAddr, product_key: &str) -> Vec<u8> {
    let v = shared.version;
    json!({
        "ip": ip.to_string(),
        "gwId": shared.dev_id.0,
        "active": 2,
        "ablilityCode": 0,
        "encrypt": v != ProtocolVersion::V31,
        "productKey": product_key,
        "version": v.as_str(),
    })
    .to_string()
    .into_bytes()
}

pub(crate) async fn spawn(
    shared: Arc<Shared>,
    cfg: BroadcastConfig,
    ip: IpAddr,
    product_key: String,
) -> io::Result<Vec<JoinHandle<()>>> {
    let payload = announcement(&shared, ip, &product_key);
    let frame = shared.suite.udp_encode(shared.version, &payload);
    let sender = UdpSocket::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)).await?;
    sender.set_broadcast(true)?;
    let sender = Arc::new(sender);
    let target = SocketAddr::new(cfg.target, cfg.port.unwrap_or(default_port(shared.version)));

    let mut tasks = Vec::new();
    {
        let shared = shared.clone();
        let sender = sender.clone();
        let frame = frame.clone();
        let interval = cfg.interval;
        tasks.push(tokio::spawn(async move {
            let mut shutdown = shared.shutdown.subscribe();
            let mut tick = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        if let Err(e) = sender.send_to(&frame, target).await {
                            tracing::debug!(error = %e, %target, "no se pudo anunciar");
                        }
                    }
                    _ = shutdown.changed() => return,
                }
            }
        }));
    }

    if let Some(port) = cfg.listen_port {
        match UdpSocket::bind(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port)).await {
            Ok(listener) => {
                let reply_port = cfg.reply_port;
                tasks.push(tokio::spawn(async move {
                    let mut shutdown = shared.shutdown.subscribe();
                    let mut buf = [0u8; 2048];
                    loop {
                        let (n, from) = tokio::select! {
                            r = listener.recv_from(&mut buf) => match r {
                                Ok(v) => v,
                                Err(_) => continue,
                            },
                            _ = shutdown.changed() => return,
                        };
                        let Some((c, body)) = shared.suite.udp_decode(&buf[..n]) else {
                            continue;
                        };
                        if c != cmd::REQ_DEVINFO {
                            continue;
                        }
                        // Contesta a la IP que dice el pedido; si no dice, a la de origen.
                        let to_ip = serde_json::from_slice::<Value>(&body)
                            .ok()
                            .and_then(|v| v.get("ip")?.as_str()?.parse::<IpAddr>().ok())
                            .unwrap_or(from.ip());
                        let to = SocketAddr::new(to_ip, reply_port);
                        tracing::debug!(%from, %to, "REQ_DEVINFO: respondo");
                        let _ = sender.send_to(&frame, to).await;
                    }
                }));
            }
            Err(e) => {
                // El 7000 puede estar tomado (en macOS lo usa AirPlay): seguimos sin él.
                tracing::warn!(error = %e, port, "no pude escuchar REQ_DEVINFO");
            }
        }
    }
    Ok(tasks)
}
