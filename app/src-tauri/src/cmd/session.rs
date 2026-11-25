//! La sesión: arrancar y detener el efecto, y reenviar sus eventos a la UI.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use color_engine::Calibration;
use openlight_core::{DeviceSpec, Session, SessionEvent, SessionOptions};
use openlight_types::{
    DeviceError, DeviceId, DeviceSessionState, ErrorCode, FixAction, HelpTopic, SessionState,
    UserFacingError,
};
use tokio::sync::broadcast::error::RecvError;

use crate::bridge::driver::TuyaDriver;
use crate::errors::{CmdResult, device_error};
use crate::events::AppEvent;
use crate::state::{ActiveSession, Backend};

/// Cuánto se espera a que el reenviador de eventos termine solo al detener.
const FORWARDER_GRACE: Duration = Duration::from_secs(1);

impl Backend {
    /// Estado actual de la sesión.
    pub fn session_state(&self) -> SessionState {
        *self
            .session_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// La calibración de un foco (sin calibración, el color pasa igual).
    pub(crate) fn load_calibration(&self, id: &DeviceId) -> Calibration {
        match self.store.load_calibration(id) {
            Ok(Some(points)) => Calibration::from_points(&points),
            Ok(None) => Calibration::identity(),
            Err(error) => {
                tracing::warn!(device = %id, %error, "calibración ilegible: se usa sin corrección");
                Calibration::identity()
            }
        }
    }

    /// Arranca el efecto con todos los focos que tienen zonas.
    ///
    /// Un foco sin clave se saltea con un aviso (`error://raised`, con "renovar clave"); si no
    /// queda ninguno, es un error. Devuelve enseguida: conectar con los focos pasa después y se
    /// ve en los eventos. Con la app cerrándose (o instalando una actualización) no hace nada,
    /// para no volver a tomar los focos que ya se devolvieron.
    pub async fn session_start(self: &Arc<Self>) -> CmdResult<SessionState> {
        let mut slot = self.session.lock().await;
        if let Some(active) = slot.as_ref() {
            return Ok(active.session.state());
        }
        if self.shutdown_state.load(Ordering::SeqCst) != 0 {
            return Ok(SessionState::Idle);
        }
        let config = self.config()?;
        let mut specs = Vec::new();
        let mut handles = Vec::new();
        let wanted = self
            .records()?
            .into_iter()
            .filter(|r| !r.zones.is_empty() && !self.is_excluded(&r.id));
        for record in wanted {
            let handle = match self.spawn_device(&record) {
                Ok(handle) => handle,
                Err(error) => {
                    self.raise(error);
                    continue;
                }
            };
            // El control manual suelta el foco: la sesión lo toma entero
            self.release_manual(&record.id);
            specs.push(DeviceSpec {
                id: record.id.clone(),
                zones: record.zones.clone(),
                settings: config.effective_settings(Some(&record.overrides))?,
                calibration: self.load_calibration(&record.id),
                driver: Box::new(TuyaDriver::new(handle.clone())),
            });
            handles.push(handle);
        }
        if specs.is_empty() {
            return Err(UserFacingError::new(ErrorCode::NoZones)
                .with_action(FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting))
                .with_action(FixAction::Retry));
        }

        let options = SessionOptions {
            power: Some(self.power.clone()),
            ..SessionOptions::default()
        };
        let session = Session::start(specs, self.sources.clone(), &self.locks, options)
            .await
            .map_err(UserFacingError::from)?;

        // Hasta que cada foco empiece a conectar la sesión todavía figura como "sin uso": para la
        // UI ya está conectando
        let state = match session.state() {
            SessionState::Idle => SessionState::Connecting,
            other => other,
        };
        self.set_session_state(state);
        self.emit(AppEvent::SessionState(
            openlight_types::ipc::SessionStateEvent { state },
        ));
        let forwarder = tokio::spawn(forward_events(self.clone(), session.subscribe(), state));
        *slot = Some(ActiveSession {
            session,
            handles,
            forwarder,
        });
        Ok(state)
    }

    /// Detiene el efecto: cada foco vuelve a su estado original y se cierra su conexión.
    pub async fn session_stop(&self) -> CmdResult<SessionState> {
        let mut slot = self.session.lock().await;
        let Some(active) = slot.take() else {
            return Ok(SessionState::Idle);
        };
        let ActiveSession {
            session,
            handles,
            forwarder,
        } = active;
        session.stop().await;

        // La sesión dejó a cada foco en su estado pero conectado: ahora se cierra del todo, y
        // se olvida el original guardado (si falla, queda para devolverlo la próxima vez).
        let mut closing = tokio::task::JoinSet::new();
        for handle in handles {
            let wait = self.options.restore_timeout;
            closing.spawn(async move {
                let id = handle.id().clone();
                match tokio::time::timeout(wait, handle.restore()).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        tracing::warn!(device = %id, %error, "el foco no volvió a su estado");
                    }
                    Err(_) => {
                        tracing::warn!(device = %id, "el foco no volvió a su estado a tiempo")
                    }
                }
            });
        }
        while closing.join_next().await.is_some() {}

        let last = match tokio::time::timeout(FORWARDER_GRACE, forwarder).await {
            Ok(Ok(state)) => state,
            _ => SessionState::Restoring,
        };
        self.set_session_state(SessionState::Idle);
        if last != SessionState::Idle {
            self.emit(AppEvent::SessionState(
                openlight_types::ipc::SessionStateEvent {
                    state: SessionState::Idle,
                },
            ));
        }
        Ok(SessionState::Idle)
    }

    /// Si hay una sesión en marcha.
    pub(crate) async fn session_running(&self) -> bool {
        self.session.lock().await.is_some()
    }

    /// Si la sesión en marcha está usando ese foco.
    pub(crate) async fn session_uses(&self, device: &DeviceId) -> bool {
        self.session
            .lock()
            .await
            .as_ref()
            .is_some_and(|s| s.handles.iter().any(|h| h.id() == device))
    }

    /// Al cerrar la app: detiene la sesión y cancela la calibración, así los focos no quedan con
    /// el color del ambiente.
    pub async fn shutdown(&self) {
        self.shutdown_state.store(1, Ordering::SeqCst);
        if let Err(error) = self.calibration_cancel().await {
            tracing::warn!(%error, "no se pudo cancelar la calibración al salir");
        }
        if let Err(error) = self.session_stop().await {
            tracing::warn!(%error, "no se pudo detener la sesión al salir");
        }
        self.shutdown_state.store(2, Ordering::SeqCst);
    }

    /// Si ya se restauraron los focos al salir.
    pub fn is_shut_down(&self) -> bool {
        self.shutdown_state.load(Ordering::SeqCst) == 2
    }

    /// Marca que alguien va a correr `shutdown`. `true` si lo marcó esta llamada (la que tiene
    /// que correrlo); `false` si otra ya lo está haciendo o ya terminó.
    pub fn begin_shutdown(&self) -> bool {
        self.shutdown_state
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Antes de instalar una actualización: el mismo cierre que "Salir" ([`Backend::shutdown`]).
    /// Tiene que pasar antes de instalar porque en Windows el instalador cierra la app sin pasar
    /// por el evento de salida.
    pub async fn update_prepare(&self) {
        self.shutdown().await;
    }

    /// La actualización no se instaló y la app sigue abierta: se puede volver a arrancar el
    /// efecto y el próximo "Salir" vuelve a restaurar los focos. Un cierre en curso no se toca.
    pub fn update_abort(&self) {
        // `Err` quiere decir que no había un cierre terminado: no hay nada que deshacer
        let _ = self
            .shutdown_state
            .compare_exchange(2, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Reenvía los eventos de la sesión a la UI. Devuelve el último estado global que vio.
async fn forward_events(
    backend: Arc<Backend>,
    mut events: tokio::sync::broadcast::Receiver<SessionEvent>,
    mut last: SessionState,
) -> SessionState {
    // Focos con un error ya avisado: el aviso sale una vez por falla, no por cada latido
    let mut raised: HashSet<DeviceId> = HashSet::new();
    loop {
        match events.recv().await {
            Ok(SessionEvent::State(event)) => {
                last = event.state;
                backend.set_session_state(last);
                backend.emit(AppEvent::SessionState(event));
            }
            Ok(SessionEvent::Device(event)) => {
                match event.state {
                    DeviceSessionState::Error(code) => {
                        if raised.insert(event.device.clone()) {
                            backend.raise(session_error(&event.device, code));
                        }
                    }
                    _ => {
                        raised.remove(&event.device);
                    }
                }
                backend.emit(AppEvent::DeviceHealth(event));
            }
            Ok(SessionEvent::Live(event)) => backend.emit(AppEvent::ColorLive(event)),
            Ok(SessionEvent::Stats(event)) => backend.emit(AppEvent::PipelineStats(event)),
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => return last,
        }
    }
}

/// El error de un foco en la sesión, con su acción.
fn session_error(device: &DeviceId, code: ErrorCode) -> UserFacingError {
    match code {
        ErrorCode::DeviceKeyRejected => device_error(device, &DeviceError::KeyRejected),
        other => UserFacingError::new(other)
            .with_param("device", device.0.clone())
            .with_action(FixAction::Retry),
    }
}
