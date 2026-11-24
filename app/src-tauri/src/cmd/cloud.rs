//! La nube de Tuya (solo hace falta al configurar y al renovar claves) y la búsqueda en la red.

use std::sync::Arc;
use std::time::Duration;

use openlight_types::ipc::{CloudCredentialsInput, CloudDeviceInfo, DiscoveredDevice};
use openlight_types::{
    Capabilities, CloudAccount, CloudCredentials, CloudError, DeviceId, Secret, TuyaCloud,
    UserFacingError,
};
use secrets::SecretKey;
use tuya_device::CapabilitiesExt;

use crate::errors::{CmdResult, invalid, no_credentials};
use crate::events::AppEvent;
use crate::state::{Backend, CloudEntry};

/// Un error de la nube listo para la UI (con la región sugerida si la hay).
pub fn cloud_error(error: &CloudError) -> UserFacingError {
    tuya_cloud::to_user_facing(error)
}

impl Backend {
    /// Valida el Access ID y el Access Secret contra la nube y, si sirven, los guarda en el
    /// llavero junto con la región.
    pub async fn cloud_validate(&self, input: CloudCredentialsInput) -> CmdResult<CloudAccount> {
        let access_id = input.access_id.trim().to_owned();
        let access_secret = input.access_secret.trim().to_owned();
        if access_id.is_empty() {
            return Err(invalid("access_id", "falta el Access ID"));
        }
        if access_secret.is_empty() {
            return Err(invalid("access_secret", "falta el Access Secret"));
        }
        let client = (self.cloud_factory)(CloudCredentials {
            access_id: access_id.clone(),
            access_secret: Secret::new(access_secret.clone()),
            region: input.region,
        })
        .map_err(|e| cloud_error(&e))?;
        let account = client.validate().await.map_err(|e| cloud_error(&e))?;

        self.secrets
            .set(&SecretKey::CloudAccessId, &Secret::new(access_id))?;
        self.secrets
            .set(&SecretKey::CloudAccessSecret, &Secret::new(access_secret))?;
        let _edit = self.edit_lock();
        let mut config = self.config()?;
        config.cloud_region = Some(account.region);
        self.store.save_config(&config)?;

        let mut cache = self.cache();
        cache.cloud = Some(client);
        cache.cloud_devices.clear();
        Ok(account)
    }

    /// El cliente de la nube: el de la última validación o uno nuevo con lo guardado.
    pub(crate) fn cloud_client(&self) -> CmdResult<Arc<dyn TuyaCloud>> {
        if let Some(client) = self.cache().cloud.clone() {
            return Ok(client);
        }
        let id = self.secrets.get(&SecretKey::CloudAccessId)?;
        let secret = self.secrets.get(&SecretKey::CloudAccessSecret)?;
        let region = self.config()?.cloud_region;
        let (Some(id), Some(secret), Some(region)) = (id, secret, region) else {
            return Err(no_credentials());
        };
        let client = (self.cloud_factory)(CloudCredentials {
            access_id: id.expose().clone(),
            access_secret: secret,
            region,
        })
        .map_err(|e| cloud_error(&e))?;
        self.cache().cloud = Some(client.clone());
        Ok(client)
    }

    /// Los dispositivos de la cuenta cruzados con lo que hay en la red, con sus capacidades.
    /// Guarda en memoria las claves y los mapeos para agregarlos después sin volver a la nube.
    pub async fn cloud_list_devices(&self) -> CmdResult<Vec<CloudDeviceInfo>> {
        let client = self.cloud_client()?;
        let devices = client.devices().await.map_err(|e| cloud_error(&e))?;

        // Si la red no se puede escuchar (permiso de Red local, firewall) la lista sale igual,
        // sin "encontrado en tu red", y el motivo llega como aviso.
        let lan = match self.scan_lan(self.options.list_scan, None).await {
            Ok(lan) => lan,
            Err(error) => {
                self.raise(error);
                Vec::new()
            }
        };
        let mut infos = tuya_discovery::match_with_cloud(&devices, &lan);

        let mut entries = Vec::new();
        for (device, info) in devices.into_iter().zip(infos.iter_mut()) {
            let mapping = if device.is_light() {
                match client.mapping(&device.id).await {
                    Ok(mapping) => Some(mapping),
                    Err(error) => {
                        tracing::warn!(device = %device.id, %error, "no se pudo leer el mapeo");
                        None
                    }
                }
            } else {
                None
            };
            info.capabilities = mapping.as_ref().and_then(Capabilities::from_mapping);
            entries.push(CloudEntry { device, mapping });
        }
        let mut cache = self.cache();
        cache.cloud_devices = entries
            .into_iter()
            .map(|e| (e.device.id.clone(), e))
            .collect();
        Ok(infos)
    }

    /// Escucha la red hasta `window` y emite `discovery://found` por cada foco nuevo.
    pub(crate) async fn scan_lan(
        &self,
        window: Duration,
        until: Option<&DeviceId>,
    ) -> CmdResult<Vec<DiscoveredDevice>> {
        let events = self.events.clone();
        let found = move |device: DiscoveredDevice| events.emit(AppEvent::DiscoveryFound(device));
        let devices = self.scanner.scan(window, until, &found).await?;
        let mut cache = self.cache();
        for device in &devices {
            cache.lan.insert(device.id.clone(), device.clone());
        }
        Ok(devices)
    }

    /// Busca focos en la red durante `timeout_s` segundos (entre 1 y 60).
    pub async fn discover_lan(&self, timeout_s: f32) -> CmdResult<Vec<DiscoveredDevice>> {
        let seconds = if timeout_s.is_finite() {
            timeout_s.clamp(1.0, 60.0)
        } else {
            5.0
        };
        self.scan_lan(Duration::from_secs_f32(seconds), None).await
    }

    /// Los datos de un dispositivo de la cuenta (clave y mapeo): los del último listado o, si
    /// no hay, los que se bajan ahora.
    pub(crate) async fn cloud_entry(&self, id: &DeviceId) -> CmdResult<CloudEntry> {
        if let Some(entry) = self.cache().cloud_devices.get(id).cloned() {
            return Ok(entry);
        }
        let client = self.cloud_client()?;
        let devices = client.devices().await.map_err(|e| cloud_error(&e))?;
        let device = devices
            .into_iter()
            .find(|d| &d.id == id)
            .ok_or_else(|| crate::errors::unknown_device(id))?;
        let mapping = if device.is_light() {
            client.mapping(id).await.ok()
        } else {
            None
        };
        let entry = CloudEntry { device, mapping };
        self.cache().cloud_devices.insert(id.clone(), entry.clone());
        Ok(entry)
    }
}
