//! Búsqueda de focos en la red local (`tuya-discovery`) detrás de un trait, para poder
//! reemplazarla en las pruebas sin abrir los puertos UDP 6666/6667/7000.

use std::time::Duration;

use async_trait::async_trait;
use openlight_types::ipc::DiscoveredDevice;
use openlight_types::{DeviceId, UserFacingError};
use tokio::sync::mpsc;
use tuya_discovery::{ScanConfig, listen, to_user_facing};

/// Quién encuentra focos en la red.
#[async_trait]
pub trait LanScanner: Send + Sync {
    /// Escucha hasta `window` y devuelve lo encontrado (uno por id, el último anuncio gana).
    /// Con `until`, termina apenas aparece ese foco. `found` se llama con cada foco la primera
    /// vez que se lo ve.
    async fn scan(
        &self,
        window: Duration,
        until: Option<&DeviceId>,
        found: &(dyn Fn(DiscoveredDevice) + Send + Sync),
    ) -> Result<Vec<DiscoveredDevice>, UserFacingError>;
}

/// Discovery UDP real.
#[derive(Debug, Clone, Default)]
pub struct UdpScanner {
    config: ScanConfig,
}

impl UdpScanner {
    /// Con otra configuración (puertos, destinos).
    pub fn with_config(config: ScanConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl LanScanner for UdpScanner {
    async fn scan(
        &self,
        window: Duration,
        until: Option<&DeviceId>,
        found: &(dyn Fn(DiscoveredDevice) + Send + Sync),
    ) -> Result<Vec<DiscoveredDevice>, UserFacingError> {
        let (tx, mut rx) = mpsc::channel(64);
        let listener = listen(&self.config, tx);
        tokio::pin!(listener);
        let deadline = tokio::time::sleep(window);
        tokio::pin!(deadline);
        let mut devices: Vec<DiscoveredDevice> = Vec::new();
        loop {
            tokio::select! {
                result = &mut listener => {
                    result.map_err(|e| to_user_facing(&e))?;
                    break;
                }
                () = &mut deadline => break,
                Some(device) = rx.recv() => {
                    if note(found, until, device, &mut devices) {
                        break;
                    }
                }
            }
        }
        while let Ok(device) = rx.try_recv() {
            note(found, until, device, &mut devices);
        }
        Ok(devices)
    }
}

/// Registra un foco visto: avisa la primera vez y dice si es el que se esperaba.
fn note(
    found: &(dyn Fn(DiscoveredDevice) + Send + Sync),
    until: Option<&DeviceId>,
    device: DiscoveredDevice,
    devices: &mut Vec<DiscoveredDevice>,
) -> bool {
    let wanted = until.is_some_and(|id| *id == device.id);
    match devices.iter_mut().find(|d| d.id == device.id) {
        Some(known) => *known = device,
        None => {
            found(device.clone());
            devices.push(device);
        }
    }
    wanted
}

/// Un escáner con una lista fija, para las pruebas.
#[derive(Debug, Clone, Default)]
pub struct FixedScanner(pub Vec<DiscoveredDevice>);

#[async_trait]
impl LanScanner for FixedScanner {
    async fn scan(
        &self,
        _window: Duration,
        _until: Option<&DeviceId>,
        found: &(dyn Fn(DiscoveredDevice) + Send + Sync),
    ) -> Result<Vec<DiscoveredDevice>, UserFacingError> {
        for device in &self.0 {
            found(device.clone());
        }
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Mutex;

    use openlight_types::ProtocolVersion;
    use tokio::net::UdpSocket;

    use super::*;

    #[tokio::test]
    async fn el_escaner_fijo_avisa_de_cada_foco() {
        let device = DiscoveredDevice {
            id: DeviceId("a".into()),
            ip: "192.0.2.5".into(),
            version: ProtocolVersion::V35,
            product_key: None,
        };
        let seen = Mutex::new(Vec::new());
        let result = FixedScanner(vec![device.clone()])
            .scan(Duration::from_secs(1), None, &|d: DiscoveredDevice| {
                seen.lock().unwrap().push(d.id.clone());
            })
            .await
            .unwrap();
        assert_eq!(result, vec![device]);
        assert_eq!(seen.into_inner().unwrap(), vec![DeviceId("a".into())]);
    }

    #[tokio::test]
    async fn el_escaner_udp_termina_en_la_ventana_si_no_hay_nadie() {
        // Un puerto efímero propio: no toca los 6666/6667/7000 de la máquina.
        let probe = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let scanner = UdpScanner::with_config(ScanConfig {
            listen_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ports: vec![port],
            request_targets: Some(Vec::new()),
            request_interval: None,
        });
        let found = scanner
            .scan(Duration::from_millis(150), None, &|_| {})
            .await
            .unwrap();
        assert!(found.is_empty());
    }
}
