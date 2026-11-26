//! La sesión sigue a la configuración mientras corre: ajustes, perfiles, zonas y qué focos
//! participan del efecto, sin detenerlo y sin cortar a los focos que siguen igual.

use std::sync::Arc;

use openlight_core::{Config, CoreError, DeviceRecord, DeviceSpec, DeviceTuning};
use openlight_types::{DeviceId, ErrorCode, UserFacingError};
use tokio::sync::{oneshot, watch};
use tuya_device::DeviceHandle;

use crate::bridge::driver::TuyaDriver;
use crate::cmd::session::close_handle;
use crate::errors::CmdResult;
use crate::state::Backend;

/// Los focos que no pudieron entrar a la sesión, con el motivo.
pub(crate) type SyncFailures = Vec<(DeviceId, UserFacingError)>;

impl Backend {
    /// Cambió la configuración: si hay una sesión en marcha, la sigue en segundo plano. Se puede
    /// llamar desde cualquier hilo; muchos cambios seguidos (un slider) se juntan en el último.
    pub(crate) fn config_changed(&self) {
        self.changes.send_modify(|n| *n = n.wrapping_add(1));
    }

    /// La próxima puesta al día vuelve a intentar con este foco, aunque haya fallado antes.
    pub(crate) async fn forget_skipped(&self, device: &DeviceId) {
        if let Some(active) = self.session.lock().await.as_mut() {
            active.skipped.remove(device);
        }
    }

    /// Los focos que participan del efecto: con zonas y sin sacar.
    pub(crate) fn effect_records(&self) -> CmdResult<Vec<DeviceRecord>> {
        Ok(self
            .records()?
            .into_iter()
            .filter(|r| !r.zones.is_empty() && !self.is_excluded(&r.id))
            .collect())
    }

    /// Lo que la sesión necesita para tomar un foco, con su actor recién lanzado.
    pub(crate) fn device_spec(
        &self,
        config: &Config,
        record: &DeviceRecord,
    ) -> CmdResult<(DeviceSpec, DeviceHandle)> {
        let handle = self.spawn_device(record)?;
        // El control manual suelta el foco: la sesión lo toma entero
        self.release_manual(&record.id);
        let spec = DeviceSpec {
            id: record.id.clone(),
            zones: record.zones.clone(),
            settings: config.effective_settings(Some(&record.overrides))?,
            calibration: self.load_calibration(&record.id),
            driver: Box::new(TuyaDriver::new(handle.clone())),
        };
        Ok((spec, handle))
    }

    /// Pone la sesión en marcha al día con la configuración: entran los focos que faltan, salen
    /// (volviendo a su estado) los que ya no participan y los demás reciben sus zonas y ajustes.
    /// Sin ningún foco que participe, la sesión se detiene. Sin sesión no hace nada.
    ///
    /// Devuelve los focos que no pudieron entrar. Uno ocupado (calibración, control manual) se
    /// vuelve a intentar con el próximo cambio; con cualquier otra falla se lo deja afuera hasta
    /// que se lo vuelva a incluir.
    pub(crate) async fn sync_session(self: &Arc<Self>) -> CmdResult<SyncFailures> {
        let mut failures = SyncFailures::new();
        // La configuración se lee con la sesión tomada: dos puestas al día no se cruzan
        let mut slot = self.session.lock().await;
        let Some(active) = slot.as_mut() else {
            return Ok(failures);
        };
        let config = self.config()?;
        let wanted = self.effect_records()?;
        let current = active.session.devices();

        // Primero los que entran: así la sesión nunca queda vacía en el medio
        for record in wanted.iter().filter(|r| !current.contains(&r.id)) {
            if active.skipped.contains(&record.id) {
                continue;
            }
            if let Some(holder) = self.locks.holder(&record.id) {
                let busy = CoreError::DeviceLocked {
                    device: record.id.clone(),
                    holder: holder.as_str().to_owned(),
                };
                failures.push((record.id.clone(), busy.into()));
                continue;
            }
            let added = self
                .device_spec(&config, record)
                .and_then(|(spec, handle)| {
                    active.session.add_device(spec)?;
                    Ok(handle)
                });
            match added {
                Ok(handle) => active.handles.push(handle),
                Err(error) => {
                    // Si lo tomaron recién, entra cuando lo suelten
                    if !is_busy(&error) {
                        active.skipped.insert(record.id.clone());
                    }
                    failures.push((record.id.clone(), error));
                }
            }
        }

        for id in current {
            if wanted.iter().any(|r| r.id == id) {
                continue;
            }
            active.session.remove_device(&id).await;
            if let Some(index) = active.handles.iter().position(|h| h.id() == &id) {
                close_handle(
                    active.handles.swap_remove(index),
                    self.options.restore_timeout,
                )
                .await;
            }
        }

        if active.session.devices().is_empty() {
            if let Some(active) = slot.take() {
                self.finish_session(active).await;
            }
            return Ok(failures);
        }

        let mut tunings = Vec::with_capacity(wanted.len());
        for record in wanted {
            tunings.push(DeviceTuning {
                settings: config.effective_settings(Some(&record.overrides))?,
                id: record.id,
                zones: record.zones,
            });
        }
        active.session.update_settings(tunings)?;
        Ok(failures)
    }
}

/// Si el foco no entró porque otra parte de la app lo está usando: se vuelve a intentar.
fn is_busy(error: &UserFacingError) -> bool {
    matches!(
        error.code,
        ErrorCode::DeviceInUse | ErrorCode::DeviceInEffect
    )
}

/// Sigue a la configuración mientras dure la sesión (`alive` se cierra con ella). También se
/// pone al día cuando se libera un foco (`releases`): así entra el que estaba ocupado.
pub(crate) async fn follow_changes(
    backend: Arc<Backend>,
    mut changes: watch::Receiver<u64>,
    mut releases: watch::Receiver<u64>,
    mut alive: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            biased;
            _ = &mut alive => return,
            changed = changes.changed() => if changed.is_err() {
                return;
            },
            released = releases.changed() => if released.is_err() {
                return;
            },
        }
        match backend.sync_session().await {
            // Un foco ocupado (calibrando, probando) no es una falla: entra más tarde
            Ok(failures) => failures
                .into_iter()
                .filter(|(_, error)| !is_busy(error))
                .for_each(|(_, error)| backend.raise(error)),
            Err(error) => backend.raise(error),
        }
    }
}
