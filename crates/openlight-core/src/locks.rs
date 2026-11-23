//! Un candado por foco: la sesión y la calibración no pueden mandarle colores al mismo tiempo
//! (`Candado` del script, pero por foco y dentro del proceso; la instancia única de la app la
//! garantiza el plugin `single-instance` de Tauri).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openlight_types::DeviceId;

use crate::error::{CoreError, Result};

/// Quién tiene un foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// La sesión de ambilight.
    Session,
    /// La pantalla de calibración.
    Calibration,
    /// Control manual y pruebas de color.
    Manual,
}

impl Holder {
    /// Nombre para mostrar en el error.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Calibration => "calibration",
            Self::Manual => "manual",
        }
    }
}

/// Reparte los candados. Se clona y comparte entre la sesión y la calibración.
#[derive(Debug, Clone, Default)]
pub struct DeviceLocks {
    held: Arc<Mutex<HashMap<DeviceId, Holder>>>,
}

impl DeviceLocks {
    /// Sin ningún foco tomado.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toma el foco, o falla con [`CoreError::DeviceLocked`] diciendo quién lo tiene.
    /// El candado se libera cuando se suelta el [`DeviceGuard`].
    pub fn try_acquire(&self, device: &DeviceId, holder: Holder) -> Result<DeviceGuard> {
        let mut held = self.lock();
        if let Some(current) = held.get(device) {
            return Err(CoreError::DeviceLocked {
                device: device.clone(),
                holder: current.as_str().to_owned(),
            });
        }
        held.insert(device.clone(), holder);
        Ok(DeviceGuard {
            locks: self.clone(),
            device: device.clone(),
        })
    }

    /// Quién tiene el foco ahora.
    pub fn holder(&self, device: &DeviceId) -> Option<Holder> {
        self.lock().get(device).copied()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<DeviceId, Holder>> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// El foco tomado. Al soltarlo (o al terminar la tarea que lo tiene) se libera.
#[derive(Debug)]
pub struct DeviceGuard {
    locks: DeviceLocks,
    device: DeviceId,
}

impl Drop for DeviceGuard {
    fn drop(&mut self) {
        self.locks.lock().remove(&self.device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str) -> DeviceId {
        DeviceId(id.to_owned())
    }

    #[test]
    fn rechaza_una_segunda_toma_y_dice_quien_lo_tiene() {
        // port: test_ambilight.py::Candado::test_rechaza_una_segunda_instancia_y_guarda_el_pid
        let locks = DeviceLocks::new();
        let _session = locks.try_acquire(&dev("fake1"), Holder::Session).unwrap();
        let error = locks
            .try_acquire(&dev("fake1"), Holder::Calibration)
            .unwrap_err();
        assert_eq!(
            error,
            CoreError::DeviceLocked {
                device: dev("fake1"),
                holder: "session".into()
            }
        );
        assert_eq!(locks.holder(&dev("fake1")), Some(Holder::Session));
    }

    #[test]
    fn se_libera_al_soltar() {
        // port: test_ambilight.py::Candado::test_se_libera_al_cerrar
        let locks = DeviceLocks::new();
        drop(locks.try_acquire(&dev("fake1"), Holder::Session).unwrap());
        assert_eq!(locks.holder(&dev("fake1")), None);
        let _again = locks
            .try_acquire(&dev("fake1"), Holder::Calibration)
            .unwrap();
    }

    #[test]
    fn cada_foco_tiene_su_candado() {
        let locks = DeviceLocks::new();
        let _a = locks.try_acquire(&dev("fake1"), Holder::Session).unwrap();
        let _b = locks
            .try_acquire(&dev("fake2"), Holder::Calibration)
            .unwrap();
    }
}
