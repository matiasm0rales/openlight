//! Estado de la sesión y eventos que ve la UI.

use std::collections::BTreeMap;
use std::sync::Mutex;

use openlight_types::ipc::{
    DeviceHealthEvent, LiveColorEvent, PipelineStatsEvent, SessionStateEvent,
};
use openlight_types::{DeviceId, DeviceSessionState, LinkHealth, SessionState};
use tokio::sync::broadcast;

/// Lo que publica la sesión. Se corresponde con los eventos del IPC (`session://state`,
/// `device://health`, `color://live`, `stats://pipeline`); la app los reenvía a la UI.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// Cambió el estado global.
    State(SessionStateEvent),
    /// Cambió el estado o la salud de un foco (también cada ~1 s mientras corre).
    Device(DeviceHealthEvent),
    /// Color de pantalla frente al enviado (≤10 Hz por foco).
    Live(LiveColorEvent),
    /// Cuadros y envíos por segundo.
    Stats(PipelineStatsEvent),
}

#[derive(Debug)]
struct Inner {
    session: SessionState,
    devices: BTreeMap<DeviceId, DeviceSessionState>,
    /// Con la sesión deteniéndose, el estado global no se recalcula.
    fixed: bool,
}

/// Estado compartido entre la sesión y las tareas de cada foco.
#[derive(Debug)]
pub(crate) struct Shared {
    inner: Mutex<Inner>,
    events: broadcast::Sender<SessionEvent>,
}

impl Shared {
    pub(crate) fn new(devices: impl IntoIterator<Item = DeviceId>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                session: SessionState::Idle,
                devices: devices
                    .into_iter()
                    .map(|d| (d, DeviceSessionState::Idle))
                    .collect(),
                fixed: false,
            }),
            events: broadcast::channel(256).0,
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    pub(crate) fn emit(&self, event: SessionEvent) {
        let _ = self.events.send(event);
    }

    pub(crate) fn session_state(&self) -> SessionState {
        self.lock().session
    }

    pub(crate) fn device_state(&self, device: &DeviceId) -> Option<DeviceSessionState> {
        self.lock().devices.get(device).copied()
    }

    /// Fija el estado global (arrancar, restaurar, terminar) y deja de recalcularlo.
    pub(crate) fn set_session(&self, state: SessionState) {
        let changed = {
            let mut inner = self.lock();
            inner.fixed = true;
            std::mem::replace(&mut inner.session, state) != state
        };
        if changed {
            self.emit(SessionEvent::State(SessionStateEvent { state }));
        }
    }

    /// Después de conectar: el estado global vuelve a seguir a los focos.
    pub(crate) fn follow_devices(&self) {
        let (changed, state) = {
            let mut inner = self.lock();
            inner.fixed = false;
            let state = aggregate(&inner.devices);
            (std::mem::replace(&mut inner.session, state) != state, state)
        };
        if changed {
            self.emit(SessionEvent::State(SessionStateEvent { state }));
        }
    }

    /// Un foco cambió de estado (o se informa su salud): publica el evento y recalcula el
    /// estado global.
    pub(crate) fn set_device(
        &self,
        device: &DeviceId,
        state: DeviceSessionState,
        health: LinkHealth,
    ) {
        let session_change = {
            let mut inner = self.lock();
            inner.devices.insert(device.clone(), state);
            if inner.fixed {
                None
            } else {
                let aggregate = aggregate(&inner.devices);
                (std::mem::replace(&mut inner.session, aggregate) != aggregate).then_some(aggregate)
            }
        };
        self.emit(SessionEvent::Device(DeviceHealthEvent {
            device: device.clone(),
            state,
            health,
        }));
        if let Some(state) = session_change {
            self.emit(SessionEvent::State(SessionStateEvent { state }));
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Estado global a partir del de cada foco: activa si alguno trabaja, en pausa si todos están
/// pausados, conectando mientras alguno conecta.
fn aggregate(devices: &BTreeMap<DeviceId, DeviceSessionState>) -> SessionState {
    use DeviceSessionState as D;
    let any = |f: fn(&D) -> bool| devices.values().any(f);
    if any(|s| matches!(s, D::Connecting)) {
        SessionState::Connecting
    } else if any(|s| matches!(s, D::Active | D::Reconnecting)) {
        SessionState::Active
    } else if !devices.is_empty() && devices.values().all(|s| matches!(s, D::Paused)) {
        SessionState::Paused
    } else if devices.values().all(|s| matches!(s, D::Idle)) {
        SessionState::Idle
    } else {
        // Todos con error o restaurando: la sesión sigue corriendo
        SessionState::Active
    }
}
