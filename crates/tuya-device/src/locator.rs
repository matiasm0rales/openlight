//! Búsqueda de la IP de un foco con discovery UDP (`tuya-discovery`).
//!
//! Es lo que hace tinytuya cuando el dispositivo se crea con `"Auto"` o `auto_ip` y la IP deja
//! de servir (`find_device`): escucha los broadcasts de los focos hasta ver el que tiene el id.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use openlight_types::DeviceId;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tuya_discovery::{ScanConfig, listen};

use crate::config::Locator;

/// Cuánto escucha cada búsqueda por defecto. Los focos anuncian cada pocos segundos y la app
/// manda un `REQ_DEVINFO` cada 6 s (que contestan los 3.5).
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(8);

/// [`Locator`] por discovery UDP.
#[derive(Debug, Clone)]
pub struct UdpLocator {
    config: ScanConfig,
    window: Duration,
}

impl UdpLocator {
    /// Escucha los puertos de siempre (6666, 6667 y 7000) durante [`DEFAULT_WINDOW`].
    pub fn new() -> Self {
        Self::with_config(ScanConfig::default(), DEFAULT_WINDOW)
    }

    /// Con otra configuración de discovery y otra ventana de escucha.
    pub fn with_config(config: ScanConfig, window: Duration) -> Self {
        Self { config, window }
    }
}

impl Default for UdpLocator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Locator for UdpLocator {
    async fn locate(&self, id: &DeviceId) -> Option<IpAddr> {
        let (tx, mut rx) = mpsc::channel(32);
        let listener = listen(&self.config, tx);
        tokio::pin!(listener);
        let deadline = sleep(self.window);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                result = &mut listener => {
                    if let Err(e) = result {
                        tracing::debug!(error = %e, "no se pudo buscar el foco por UDP");
                    }
                    return None;
                }
                () = &mut deadline => return None,
                Some(found) = rx.recv() => {
                    if found.id == *id {
                        match found.ip.parse() {
                            Ok(ip) => return Some(ip),
                            Err(e) => tracing::debug!(ip = %found.ip, error = %e, "IP anunciada inválida"),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use openlight_types::{DeviceId, ProtocolVersion};
    use tuya_sim::{BroadcastConfig, SimConfig};

    use super::*;

    fn free_udp_port() -> u16 {
        std::net::UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// Un foco simulado que anuncia por loopback en `port`, y el locator que lo escucha.
    async fn escenario(version: ProtocolVersion) -> (tuya_sim::SimHandle, UdpLocator) {
        let port = free_udp_port();
        let sim = tuya_sim::start(
            SimConfig::new(version)
                .ephemeral()
                .broadcast(BroadcastConfig {
                    target: Ipv4Addr::LOCALHOST.into(),
                    port: Some(port),
                    interval: Duration::from_millis(50),
                    advertised_ip: None,
                    listen_port: None,
                    reply_port: 0,
                }),
        )
        .await
        .unwrap();
        let config = ScanConfig {
            listen_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ports: vec![port],
            request_targets: Some(Vec::new()),
            request_interval: None,
        };
        (sim, UdpLocator::with_config(config, Duration::from_secs(3)))
    }

    #[tokio::test]
    async fn encuentra_la_ip_de_donde_viene_el_anuncio() {
        // Como en tinytuya, la IP es la de origen del datagrama (acá, el loopback).
        for version in [ProtocolVersion::V33, ProtocolVersion::V35] {
            let (sim, locator) = escenario(version).await;
            let ip = locator.locate(sim.dev_id()).await;
            assert_eq!(ip, Some(Ipv4Addr::LOCALHOST.into()), "{version}");
        }
    }

    #[tokio::test]
    async fn ignora_los_focos_de_otros() {
        let (_sim, locator) = escenario(ProtocolVersion::V33).await;
        let locator = UdpLocator {
            window: Duration::from_millis(400),
            ..locator
        };
        assert_eq!(locator.locate(&DeviceId("otro-foco".into())).await, None);
    }

    #[tokio::test]
    async fn sin_anuncios_se_rinde_al_cumplir_la_ventana() {
        let config = ScanConfig {
            listen_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ports: vec![free_udp_port()],
            request_targets: Some(Vec::new()),
            request_interval: None,
        };
        let locator = UdpLocator::with_config(config, Duration::from_millis(200));
        assert_eq!(locator.locate(&DeviceId("nadie".into())).await, None);
    }
}
