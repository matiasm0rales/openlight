//! `LightDriver` (lo que pide el pipeline de `openlight-core`) sobre el actor de `tuya-device`.

use async_trait::async_trait;
use openlight_core::LightDriver;
use openlight_types::{DeviceError, Hsv, LinkHealth};
use tuya_device::DeviceHandle;

/// Un foco de la sesión: el actor de `tuya-device` visto como [`LightDriver`].
///
/// - `prepare` encuentra el foco y guarda su estado original la primera vez (`start`), y en
///   cada llamada lo deja en modo color (`activate`). Después de una pausa el actor sigue
///   conectado, así que volver solo necesita `activate`.
/// - `send_hsv` no espera: el actor coalesce y manda solo el último color.
/// - `restore` devuelve el foco a su estado y lo deja conectado para retomar
///   (`return_to_original`). El cierre definitivo (`DeviceHandle::restore`) lo hace la app
///   cuando termina la sesión.
pub struct TuyaDriver {
    handle: DeviceHandle,
    started: bool,
}

impl TuyaDriver {
    /// Un driver sobre `handle` (todavía sin conectar).
    pub fn new(handle: DeviceHandle) -> Self {
        Self {
            handle,
            started: false,
        }
    }
}

#[async_trait]
impl LightDriver for TuyaDriver {
    async fn prepare(&mut self) -> Result<(), DeviceError> {
        if !self.started {
            self.handle.start().await?;
            self.started = true;
        }
        self.handle.activate().await
    }

    async fn send_hsv(&mut self, color: Hsv) -> Result<(), DeviceError> {
        self.handle.send_colour(color);
        Ok(())
    }

    async fn restore(&mut self) -> Result<bool, DeviceError> {
        self.handle.return_to_original().await.map(|()| true)
    }

    fn health(&self) -> LinkHealth {
        self.handle.health()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::time::Duration;

    use openlight_types::{Capabilities, DeviceCreds, ProtocolVersion, Secret};
    use tuya_device::{CapabilitiesExt, DeviceConfig, IpMode, MemoryStore, NoLocator, spawn};
    use tuya_sim::{SimConfig, SimHandle};

    use super::*;

    async fn driver_for(sim: &SimHandle) -> (TuyaDriver, Capabilities) {
        let caps = Capabilities::from_mapping(sim.mapping()).expect("el simulador es una luz");
        let creds = DeviceCreds {
            id: sim.dev_id().clone(),
            local_key: Secret::new(String::from_utf8(sim.local_key().to_vec()).unwrap()),
            version: ProtocolVersion::V35,
            ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        };
        let config = DeviceConfig::new(creds)
            .capabilities(caps.clone())
            .ip_mode(IpMode::Fixed)
            .port(sim.port());
        let handle = spawn(config, Arc::new(NoLocator), Arc::new(MemoryStore::new()));
        (TuyaDriver::new(handle), caps)
    }

    async fn until(mut cond: impl FnMut() -> bool) {
        for _ in 0..150 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no se cumplió a tiempo");
    }

    #[tokio::test]
    async fn prepara_manda_el_color_y_devuelve_el_foco_a_su_estado() {
        let sim = tuya_sim::start(SimConfig::new(ProtocolVersion::V35).ephemeral())
            .await
            .unwrap();
        let (mut driver, caps) = driver_for(&sim).await;
        let mode = caps.dp_mode.clone().unwrap();
        let colour = caps.dp_colour.clone().unwrap().0;
        let original_mode = sim.dp(&mode).unwrap();
        let original_colour = sim.dp(&colour).unwrap();

        driver.prepare().await.unwrap();
        assert!(driver.health().connected);
        assert_eq!(
            sim.dp(&mode).unwrap(),
            "colour",
            "prepare deja el modo color"
        );

        driver.send_hsv(Hsv::new(0.33, 1.0, 1.0)).await.unwrap();
        until(|| sim.dp(&colour).unwrap() != original_colour).await;

        assert!(driver.restore().await.unwrap());
        assert_eq!(
            sim.dp(&mode).unwrap(),
            original_mode,
            "vuelve al modo original"
        );
        assert!(driver.health().connected, "la pausa no corta la conexión");

        // Volver de la pausa solo necesita `activate`: no vuelve a guardar el original
        driver.prepare().await.unwrap();
        assert_eq!(sim.dp(&mode).unwrap(), "colour");
        sim.shutdown().await;
    }
}
