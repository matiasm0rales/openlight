//! Eventos del backend hacia la UI.
//!
//! El backend no conoce a Tauri: emite [`AppEvent`] a un [`EventSink`]. En la app real el sink es
//! [`TauriSink`] (que hace `emit` con los nombres de `openlight_types::ipc::events`); en las pruebas
//! es un [`MemorySink`] que guarda todo lo emitido.

use std::sync::{Arc, Mutex, PoisonError};

use openlight_types::UserFacingError;
use openlight_types::ipc::{
    DeviceHealthEvent, DiscoveredDevice, EffectDevicesEvent, LiveColorEvent, PipelineStatsEvent,
    SessionStateEvent, events,
};
use tauri::Emitter;
use tokio::sync::broadcast;

/// Un evento del contrato IPC con su payload.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// `session://state`.
    SessionState(SessionStateEvent),
    /// `device://health`.
    DeviceHealth(DeviceHealthEvent),
    /// `color://live`.
    ColorLive(LiveColorEvent),
    /// `stats://pipeline`.
    PipelineStats(PipelineStatsEvent),
    /// `error://raised`.
    ErrorRaised(UserFacingError),
    /// `discovery://found`.
    DiscoveryFound(DiscoveredDevice),
    /// `effect://devices`.
    EffectDevices(EffectDevicesEvent),
}

impl AppEvent {
    /// Nombre del evento en el IPC.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SessionState(_) => events::SESSION_STATE,
            Self::DeviceHealth(_) => events::DEVICE_HEALTH,
            Self::ColorLive(_) => events::COLOR_LIVE,
            Self::PipelineStats(_) => events::PIPELINE_STATS,
            Self::ErrorRaised(_) => events::ERROR_RAISED,
            Self::DiscoveryFound(_) => events::DISCOVERY_FOUND,
            Self::EffectDevices(_) => events::EFFECT_DEVICES,
        }
    }
}

/// A dónde van los eventos.
pub trait EventSink: Send + Sync {
    /// Publica un evento. No bloquea y nunca falla: sin nadie escuchando no pasa nada.
    fn emit(&self, event: AppEvent);
}

/// El sink de la app real: cada evento sale por `AppHandle::emit` hacia todas las ventanas.
pub struct TauriSink<R: tauri::Runtime = tauri::Wry>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> EventSink for TauriSink<R> {
    fn emit(&self, event: AppEvent) {
        let name = event.name();
        let sent = match event {
            AppEvent::SessionState(p) => self.0.emit(name, p),
            AppEvent::DeviceHealth(p) => self.0.emit(name, p),
            AppEvent::ColorLive(p) => self.0.emit(name, p),
            AppEvent::PipelineStats(p) => self.0.emit(name, p),
            AppEvent::ErrorRaised(p) => self.0.emit(name, p),
            AppEvent::DiscoveryFound(p) => self.0.emit(name, p),
            AppEvent::EffectDevices(p) => self.0.emit(name, p),
        };
        if let Err(error) = sent {
            tracing::debug!(event = name, %error, "no se pudo emitir el evento");
        }
    }
}

/// Un sink que guarda lo emitido y además lo reparte por un canal, para las pruebas.
#[derive(Debug, Clone)]
pub struct MemorySink {
    log: Arc<Mutex<Vec<AppEvent>>>,
    tx: broadcast::Sender<AppEvent>,
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySink {
    /// Un sink vacío.
    pub fn new() -> Self {
        Self {
            log: Arc::default(),
            tx: broadcast::channel(1024).0,
        }
    }

    /// Todo lo emitido hasta ahora, en orden.
    pub fn events(&self) -> Vec<AppEvent> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Los eventos que se emitan de ahora en adelante.
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl EventSink for MemorySink {
    fn emit(&self, event: AppEvent) {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.clone());
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use openlight_types::SessionState;

    use super::*;

    #[test]
    fn cada_evento_usa_su_nombre_del_contrato() {
        let event = AppEvent::SessionState(SessionStateEvent {
            state: SessionState::Idle,
        });
        assert_eq!(event.name(), "session://state");
    }

    #[test]
    fn el_sink_en_memoria_guarda_y_reparte() {
        let sink = MemorySink::new();
        let mut rx = sink.subscribe();
        let event = AppEvent::SessionState(SessionStateEvent {
            state: SessionState::Active,
        });
        sink.emit(event.clone());
        assert_eq!(sink.events(), vec![event.clone()]);
        assert_eq!(rx.try_recv().unwrap(), event);
    }
}
