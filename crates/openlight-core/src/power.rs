//! Suspensión y despertar del sistema.
//!
//! La integración nativa (macOS `NSWorkspace`, Windows `WM_POWERBROADCAST`, Linux logind
//! `PrepareForSleep`) vive en la app y llama a [`PowerEvents::emit`]. Acá solo está el bus de
//! eventos y el trait que esa integración implementa, para poder probar la lógica de
//! "restaurar antes de dormir, reanudar al despertar" con un evento falso.

use tokio::sync::broadcast;

use crate::error::Result;

/// Qué le pasa a la energía del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// El sistema está por suspenderse: hay que dejar los focos como estaban.
    Suspend,
    /// El sistema despertó: hay que retomar.
    Resume,
}

/// Bus de eventos de energía. Se clona: quien detecta el evento llama a `emit` y la sesión
/// escucha con `subscribe`.
#[derive(Debug, Clone)]
pub struct PowerEvents {
    tx: broadcast::Sender<PowerEvent>,
}

impl Default for PowerEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerEvents {
    /// Un bus nuevo.
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(16).0,
        }
    }

    /// Publica un evento. Sin nadie escuchando no pasa nada.
    pub fn emit(&self, event: PowerEvent) {
        let _ = self.tx.send(event);
    }

    /// Escucha los eventos.
    pub fn subscribe(&self) -> broadcast::Receiver<PowerEvent> {
        self.tx.subscribe()
    }
}

/// Integración nativa con el sistema operativo (la implementa la app, una por SO).
pub trait NativePower: Send {
    /// Empieza a escuchar al sistema y publica en `events`. No debe bloquear.
    fn install(&mut self, events: PowerEvents) -> Result<()>;
}

/// Sin integración nativa (por ejemplo, en pruebas o en un SO sin soporte todavía).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoNativePower;

impl NativePower for NoNativePower {
    fn install(&mut self, _events: PowerEvents) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn los_eventos_llegan_a_todos_los_oyentes() {
        let bus = PowerEvents::new();
        let mut a = bus.subscribe();
        let mut b = bus.clone().subscribe();
        bus.emit(PowerEvent::Suspend);
        bus.emit(PowerEvent::Resume);
        assert_eq!(a.recv().await.unwrap(), PowerEvent::Suspend);
        assert_eq!(a.recv().await.unwrap(), PowerEvent::Resume);
        assert_eq!(b.recv().await.unwrap(), PowerEvent::Suspend);
    }

    #[test]
    fn emitir_sin_oyentes_no_falla() {
        PowerEvents::new().emit(PowerEvent::Suspend);
    }

    #[test]
    fn sin_integracion_nativa_instala_sin_hacer_nada() {
        NoNativePower.install(PowerEvents::new()).unwrap();
    }
}
