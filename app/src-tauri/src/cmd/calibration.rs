//! Calibración de un foco (reemplaza a `calibrar.py`).

use openlight_core::DeviceGuard;
use openlight_types::ipc::CalibrationPoint;
use openlight_types::{DeviceId, DisplayId};
use tuya_device::DeviceHandle;

/// Una calibración en curso: el foco conectado, su candado y la tabla que se va armando.
pub(crate) struct CalibrationRun {
    pub device: DeviceId,
    pub display: DisplayId,
    pub points: Vec<CalibrationPoint>,
    pub index: u32,
    pub handle: DeviceHandle,
    pub _guard: DeviceGuard,
}

impl crate::state::Backend {
    /// Cierra la calibración sin guardar (se completa en el commit de calibración).
    pub async fn calibration_cancel(&self) -> crate::errors::CmdResult<()> {
        self.calibration.lock().await.take();
        Ok(())
    }
}
