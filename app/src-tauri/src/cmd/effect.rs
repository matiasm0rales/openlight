//! Qué focos participan del efecto (el tilde de cada foco en el menú del tray).

use std::collections::HashSet;
use std::sync::Arc;

use openlight_types::{DeviceId, ErrorCode};

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

    /// Prende o apaga un foco dentro del efecto. Con la sesión en marcha, la reinicia para que
    /// el cambio se vea ya; si no queda ningún foco, simplemente la detiene.
    pub async fn toggle_device_in_effect(self: &Arc<Self>, device: DeviceId) -> CmdResult<()> {
        self.record(&device)?;
        {
            let mut excluded = self
                .excluded
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !excluded.remove(&device) {
                excluded.insert(device);
            }
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
        self.notify_changed();
        if self.session_running().await {
            self.session_stop().await?;
            match self.session_start().await {
                Err(error) if error.code == ErrorCode::NoZones => {}
                other => {
                    other?;
                }
            }
        }
        Ok(())
    }
}
