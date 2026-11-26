//! La tarea de un foco: mueve su [`DeviceEngine`] con el reloj y los cuadros reales y ejecuta
//! las acciones sobre el [`LightDriver`]. Que un foco se caiga o tarde no frena a los demás:
//! cada uno tiene su tarea.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use openlight_types::ipc::LiveColorEvent;
use openlight_types::{
    DeviceError, DeviceId, DeviceSessionState, DisplayId, ErrorCode, Frame, LinkHealth, Settings,
    Zone,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, Interval, MissedTickBehavior, interval};

use super::shared::{SessionEvent, Shared};
use crate::driver::LightDriver;
use crate::engine::{Action, DeviceEngine, Link};
use crate::locks::DeviceGuard;
use crate::pipeline::Frames;

/// Cada cuánto se publica el color en vivo de un foco (≤10 Hz).
const LIVE_EVERY: Duration = Duration::from_millis(100);
/// Cada cuánto se publica la salud del enlace aunque no haya cambios de estado.
const HEALTH_EVERY: Duration = Duration::from_secs(1);

/// Intentos de devolver el foco a su estado al detener.
const RESTORE_ATTEMPTS: u32 = 3;
/// Espera entre esos intentos.
const RESTORE_RETRY_EVERY: Duration = Duration::from_millis(500);

/// Órdenes que recibe la tarea de un foco.
#[derive(Debug)]
pub(crate) enum Command {
    /// El sistema se va a dormir: restaurar el foco y avisar cuando termine.
    Suspend(oneshot::Sender<()>),
    /// El sistema despertó.
    Resume,
    /// Fin de la sesión: restaurar el foco y avisar cuando termine.
    Stop(oneshot::Sender<()>),
}

/// Lo que se le puede cambiar a la tarea de un foco sin soltarlo. Llega por un `watch`: nunca
/// bloquea a la sesión y, si la tarea está ocupada, vale lo último.
#[derive(Clone)]
pub(crate) struct Tuning {
    pub zones: Vec<Zone>,
    pub settings: Settings,
    /// Los cuadros de cada monitor que usan las zonas.
    pub receivers: HashMap<DisplayId, watch::Receiver<Option<Frame>>>,
}

pub(crate) fn retry_every(settings: &Settings) -> Duration {
    Duration::from_secs_f64(f64::from(settings.retry_every_s))
}

/// El reloj de pasos de un foco.
fn frame_ticker(period_s: f64) -> Interval {
    let mut ticker = interval(Duration::from_secs_f64(period_s));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticker
}

pub(crate) struct DeviceRunner {
    pub id: DeviceId,
    pub engine: DeviceEngine,
    pub driver: Box<dyn LightDriver>,
    pub receivers: HashMap<DisplayId, watch::Receiver<Option<Frame>>>,
    pub shared: Arc<Shared>,
    pub started: Instant,
    pub retry_every: Duration,
    /// Envíos de color, para `stats://pipeline`.
    pub sends: Arc<AtomicU32>,
    /// Mientras la tarea viva, el foco es de la sesión.
    pub _guard: DeviceGuard,
    pub state: DeviceSessionState,
    pub last_live: Option<Instant>,
    pub last_health: Option<Instant>,
    pub retry_at: Option<Instant>,
}

impl DeviceRunner {
    pub(crate) async fn run(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        mut tuning: watch::Receiver<Tuning>,
    ) {
        self.begin().await;
        let mut ticker = frame_ticker(self.engine.timing().frame_period_s);
        loop {
            tokio::select! {
                Ok(()) = tuning.changed() => {
                    let period = self.engine.timing().frame_period_s;
                    let new = tuning.borrow_and_update().clone();
                    self.retune(new);
                    if self.engine.timing().frame_period_s != period {
                        ticker = frame_ticker(self.engine.timing().frame_period_s);
                    }
                }
                _ = ticker.tick() => self.step().await,
                command = commands.recv() => match command {
                    Some(Command::Suspend(done)) => {
                        self.suspend().await;
                        let _ = done.send(());
                    }
                    Some(Command::Resume) => self.engine.resume(),
                    Some(Command::Stop(done)) => {
                        self.finish().await;
                        let _ = done.send(());
                        return;
                    }
                    // La sesión se descartó sin parar: el foco igual vuelve a su estado
                    None => {
                        self.finish().await;
                        return;
                    }
                },
            }
        }
    }

    fn retune(&mut self, tuning: Tuning) {
        self.engine.apply_settings(&tuning.settings);
        self.engine.set_zones(tuning.zones);
        self.receivers = tuning.receivers;
        self.retry_every = retry_every(&tuning.settings);
        self.driver.apply_settings(&tuning.settings);
    }

    fn now_s(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn link(&self) -> (LinkHealth, Link) {
        let health = self.driver.health();
        let link = Link {
            connected: health.connected,
            reconnects: health.reconnects,
        };
        (health, link)
    }

    fn set_state(&mut self, state: DeviceSessionState) {
        let health = self.driver.health();
        self.state = state;
        self.last_health = Some(Instant::now());
        self.shared.set_device(&self.id, state, health);
    }

    /// Estado de trabajo según el enlace: activo si hay conexión, si no reconectando.
    fn working_state(connected: bool) -> DeviceSessionState {
        if connected {
            DeviceSessionState::Active
        } else {
            DeviceSessionState::Reconnecting
        }
    }

    async fn begin(&mut self) {
        self.set_state(DeviceSessionState::Connecting);
        let (_, link) = self.link();
        let actions = self.engine.start(link);
        self.execute(actions).await;
    }

    async fn step(&mut self) {
        if let Some(at) = self.retry_at {
            if Instant::now() < at {
                return;
            }
            self.retry_at = None;
        }
        let frames: Frames = self
            .receivers
            .iter()
            .filter_map(|(display, rx)| rx.borrow().clone().map(|f| (display.clone(), f)))
            .collect();
        let (health, link) = self.link();
        let step = self.engine.tick(self.now_s(), &frames, link);
        self.execute(step.actions).await;

        // El estado sigue al enlace mientras el foco trabaja
        if matches!(
            self.state,
            DeviceSessionState::Active | DeviceSessionState::Reconnecting
        ) {
            let wanted = Self::working_state(health.connected);
            if wanted != self.state {
                self.set_state(wanted);
            }
        }
        if let Some(processed) = step.processed
            && self.last_live.is_none_or(|t| t.elapsed() >= LIVE_EVERY)
        {
            self.last_live = Some(Instant::now());
            self.shared.emit(SessionEvent::Live(LiveColorEvent {
                device: self.id.clone(),
                screen: processed.screen,
                sent: self.engine.sent(),
            }));
        }
        if self.last_health.is_none_or(|t| t.elapsed() >= HEALTH_EVERY) {
            self.set_state(self.state);
        }
    }

    async fn suspend(&mut self) {
        let actions = self.engine.suspend();
        self.execute(actions).await;
        if self.state != DeviceSessionState::Paused {
            self.set_state(DeviceSessionState::Paused);
        }
    }

    async fn finish(&mut self) {
        self.set_state(DeviceSessionState::Restoring);
        // Si la restauración falla se reintenta unas veces antes de rendirse: es lo que deja
        // al foco con el color del ambiente en vez de su estado original
        for attempt in 0..RESTORE_ATTEMPTS {
            let actions = self.engine.stop();
            if actions.is_empty() {
                break;
            }
            self.execute(actions).await;
            if self.engine.is_restored() {
                break;
            }
            if attempt + 1 < RESTORE_ATTEMPTS {
                tokio::time::sleep(RESTORE_RETRY_EVERY).await;
            }
        }
        self.set_state(DeviceSessionState::Idle);
    }

    async fn execute(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Prepare => self.prepare().await,
                Action::Send(color) => match self.driver.send_hsv(color).await {
                    Ok(()) => {
                        self.sends.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        tracing::debug!(device = %self.id, %error, "no se pudo enviar el color");
                        self.engine.send_failed();
                        self.note_error(&error);
                    }
                },
                Action::Restore => {
                    if let Err(error) = self.driver.restore().await {
                        tracing::warn!(device = %self.id, %error, "no se pudo devolver el foco a su estado");
                        // Sigue pendiente: se reintenta pasado `retry_every` o al detener
                        self.engine.restore_failed();
                        self.retry_at = Some(Instant::now() + self.retry_every);
                    }
                    if self.state != DeviceSessionState::Restoring {
                        self.set_state(DeviceSessionState::Paused);
                    }
                }
            }
        }
    }

    async fn prepare(&mut self) {
        if self.state == DeviceSessionState::Paused {
            self.set_state(DeviceSessionState::Connecting);
        }
        match self.driver.prepare().await {
            Ok(()) => {
                let connected = self.driver.health().connected;
                self.set_state(Self::working_state(connected));
            }
            Err(error) => {
                tracing::warn!(device = %self.id, %error, "no se pudo preparar el foco");
                self.engine.prepare_failed();
                self.retry_at = Some(Instant::now() + self.retry_every);
                self.note_error(&error);
            }
        }
    }

    /// Un error que el usuario tiene que resolver (la clave) se muestra; el resto es
    /// transitorio y el actor del foco se ocupa de reconectar.
    fn note_error(&mut self, error: &DeviceError) {
        let state = match error {
            DeviceError::KeyRejected => DeviceSessionState::Error(ErrorCode::DeviceKeyRejected),
            _ => DeviceSessionState::Reconnecting,
        };
        if state != self.state {
            self.set_state(state);
        }
    }
}
