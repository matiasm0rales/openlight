//! Qué focos participan del efecto (el tilde de cada foco en el menú del tray).

use std::collections::HashSet;
use std::sync::Arc;

use openlight_types::DeviceId;

use crate::errors::{CmdResult, internal};
use crate::state::Backend;

const FILE: &str = "excluded_devices.json";

impl Backend {
    /// Los focos que el usuario sacó del efecto, guardados para el próximo arranque.
    pub(crate) fn load_excluded(&self) -> HashSet<DeviceId> {
        std::fs::read_to_string(self.store.dir().join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<DeviceId>>(&text).ok())
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Si el foco está fuera del efecto.
    pub fn is_excluded(&self, device: &DeviceId) -> bool {
        self.excluded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(device)
    }

    /// El tilde de un foco en el menú del tray: lo saca del efecto o lo vuelve a incluir.
    pub async fn toggle_device_in_effect(self: &Arc<Self>, device: DeviceId) -> CmdResult<()> {
        let include = self.is_excluded(&device);
        self.set_device_in_effect(device, include).await
    }

    /// Incluye un foco en el efecto o lo saca, y lo recuerda. Con la sesión en marcha vale ya y
    /// sin cortar a los demás: al sacarlo, cuando esto vuelve el foco ya está como antes de
    /// empezar y libre (para el control manual o la calibración). Sin ningún foco que participe,
    /// la sesión se detiene.
    pub async fn set_device_in_effect(
        self: &Arc<Self>,
        device: DeviceId,
        in_effect: bool,
    ) -> CmdResult<()> {
        self.record(&device)?;
        if !self.save_excluded(&device, !in_effect)? {
            return Ok(());
        }
        self.notify_changed();
        if in_effect {
            self.forget_skipped(&device).await;
        }
        let failures = self.sync_session().await?;
        // Si no pudo entrar (lo tiene la calibración, por ejemplo), queda afuera y se dice por qué
        if in_effect && let Some((_, error)) = failures.into_iter().find(|(id, _)| id == &device) {
            self.save_excluded(&device, true)?;
            self.notify_changed();
            return Err(error);
        }
        Ok(())
    }

    /// Anota si el foco está fuera del efecto y lo guarda. `false` si ya estaba así.
    fn save_excluded(&self, device: &DeviceId, out: bool) -> CmdResult<bool> {
        let mut excluded = self
            .excluded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed = if out {
            excluded.insert(device.clone())
        } else {
            excluded.remove(device)
        };
        if changed {
            let mut ids: Vec<&DeviceId> = excluded.iter().collect();
            ids.sort();
            let text = serde_json::to_string(&ids).map_err(|e| internal(e.to_string()))?;
            std::fs::write(self.store.dir().join(FILE), text).map_err(|e| {
                openlight_core::CoreError::Io {
                    path: self.store.dir().join(FILE),
                    detail: e.to_string(),
                }
            })?;
        }
        Ok(changed)
    }
}
