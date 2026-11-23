//! La sesión: arma el pipeline captura → color → focos y lo mantiene andando.
//!
//! ```text
//!  ScreenSource (uno por monitor usado) ──watch──┐
//!                                                ├─► tarea de cada foco ─► DeviceEngine ─► LightDriver
//!  ScreenSource ─────────────────────────watch───┘     (zonas, color, límites, pausa)
//! ```
//!
//! - **Una captura por monitor usado**, compartida por todos los focos que tienen una zona ahí.
//! - **Cada foco tiene su tarea**, su calibración, su suavizador y su límite de envíos.
//! - Con la sesión detenida, dormida o sin pantalla, cada foco vuelve a su estado.
//!
//! Siempre hay que terminar con [`Session::stop`]: es lo que espera a que los focos vuelvan a
//! su estado original.

mod runner;
mod shared;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use color_engine::Calibration;
use openlight_types::ipc::{DeviceRate, PipelineStatsEvent};
use openlight_types::{
    DeviceId, DeviceSessionState, DisplayId, ScreenSource, SessionState, Settings, Zone,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::driver::LightDriver;
use crate::engine::{DeviceEngine, Timing};
use crate::error::{CoreError, Result};
use crate::locks::{DeviceGuard, DeviceLocks, Holder};
use crate::pipeline::ColorPipeline;
use crate::power::{PowerEvent, PowerEvents};
use runner::{Command, DeviceRunner};
pub use shared::SessionEvent;
use shared::Shared;

#[cfg(test)]
mod tests;

/// Crea una captura nueva. La sesión lo llama una vez por monitor usado.
pub type SourceFactory = Arc<dyn Fn() -> Box<dyn ScreenSource> + Send + Sync>;

/// Tamaño al que se reduce cada cuadro (`~96×54` en el plan).
pub const CAPTURE_SIZE: (u32, u32) = (96, 54);

/// Lo que la sesión necesita de un foco.
pub struct DeviceSpec {
    /// Id del foco.
    pub id: DeviceId,
    /// Las zonas que sigue (al menos una).
    pub zones: Vec<Zone>,
    /// Ajustes efectivos del foco (globales ← perfil ← lo propio del foco).
    pub settings: Settings,
    /// Su calibración (`Calibration::identity()` si no tiene).
    pub calibration: Calibration,
    /// El foco.
    pub driver: Box<dyn LightDriver>,
}

/// Opciones de la sesión.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Tamaño al que se reducen los cuadros.
    pub capture_size: (u32, u32),
    /// Eventos de energía del sistema, si se quiere restaurar antes de dormir.
    pub power: Option<PowerEvents>,
    /// Cuánto se espera a que los focos vuelvan a su estado al detener (`ESPERA_DETENER`).
    pub stop_timeout: Duration,
    /// Cuánto se espera al suspender: el sistema da pocos segundos.
    pub suspend_timeout: Duration,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            capture_size: CAPTURE_SIZE,
            power: None,
            stop_timeout: Duration::from_secs(12),
            suspend_timeout: Duration::from_secs(5),
        }
    }
}

struct DeviceTask {
    id: DeviceId,
    commands: mpsc::Sender<Command>,
    join: JoinHandle<()>,
}

/// Una sesión en marcha.
pub struct Session {
    shared: Arc<Shared>,
    devices: Vec<DeviceTask>,
    sources: Vec<Box<dyn ScreenSource>>,
    /// Estadísticas, contadores de cuadros y escucha de energía.
    helpers: Vec<JoinHandle<()>>,
    options: SessionOptions,
}

impl Session {
    /// Arranca la sesión: toma los focos, empieza una captura por monitor usado y lanza la
    /// tarea de cada foco.
    ///
    /// Falla sin dejar nada a medias si un foco ya está en uso (calibración u otra sesión), si
    /// una zona es inválida o si no se puede capturar un monitor.
    pub async fn start(
        devices: Vec<DeviceSpec>,
        sources: SourceFactory,
        locks: &DeviceLocks,
        options: SessionOptions,
    ) -> Result<Self> {
        let devices: Vec<DeviceSpec> = devices
            .into_iter()
            .filter(|d| !d.zones.is_empty())
            .collect();
        if devices.is_empty() {
            return Err(CoreError::NothingToDo);
        }
        for zone in devices.iter().flat_map(|d| &d.zones) {
            if !zone.rect.is_valid() {
                return Err(CoreError::Invalid(format!(
                    "zona fuera del monitor {}: {:?}",
                    zone.display, zone.rect
                )));
            }
        }

        // Si algo falla de acá en adelante, soltar los candados (los guardas) es suficiente
        let mut guards: BTreeMap<DeviceId, DeviceGuard> = BTreeMap::new();
        for device in &devices {
            let guard = locks.try_acquire(&device.id, Holder::Session)?;
            guards.insert(device.id.clone(), guard);
        }

        let shared = Arc::new(Shared::new(devices.iter().map(|d| d.id.clone())));
        shared.set_session(SessionState::Connecting);

        // Una captura por monitor, a los cuadros por segundo del foco más exigente
        let mut fps: BTreeMap<DisplayId, f32> = BTreeMap::new();
        for device in &devices {
            for zone in &device.zones {
                let entry = fps.entry(zone.display.clone()).or_insert(0.0);
                *entry = entry.max(device.settings.capture_fps);
            }
        }
        let mut started: Vec<Box<dyn ScreenSource>> = Vec::new();
        let mut receivers = HashMap::new();
        for (display, fps) in &fps {
            let mut source = sources();
            match source.start(display.clone(), options.capture_size, *fps) {
                Ok(rx) => {
                    receivers.insert(display.clone(), rx);
                    started.push(source);
                }
                Err(error) => {
                    for mut s in started {
                        s.stop();
                    }
                    shared.set_session(SessionState::Idle);
                    return Err(error.into());
                }
            }
        }

        let started_at = Instant::now();
        let mut tasks = Vec::new();
        let mut helpers = Vec::new();
        let mut counters: Vec<(DeviceId, Arc<AtomicU32>)> = Vec::new();
        for device in devices {
            let rx_map = device
                .zones
                .iter()
                .map(|z| z.display.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|d| receivers.get(&d).map(|rx| (d, rx.clone())))
                .collect();
            let sends = Arc::new(AtomicU32::new(0));
            counters.push((device.id.clone(), sends.clone()));
            let timing = Timing::from_settings(&device.settings);
            let pipeline = ColorPipeline::new(device.zones, &device.settings, device.calibration);
            let guard = guards.remove(&device.id).expect("candado tomado arriba");
            let runner = DeviceRunner {
                id: device.id.clone(),
                engine: DeviceEngine::new(pipeline, timing),
                driver: device.driver,
                receivers: rx_map,
                shared: shared.clone(),
                started: started_at,
                retry_every: Duration::from_secs_f64(f64::from(device.settings.retry_every_s)),
                sends,
                _guard: guard,
                state: DeviceSessionState::Idle,
                last_live: None,
                last_health: None,
                retry_at: None,
            };
            let (tx, rx) = mpsc::channel(8);
            tasks.push(DeviceTask {
                id: device.id,
                commands: tx,
                join: tokio::spawn(runner.run(rx)),
            });
        }

        // Cuadros por segundo que llegan de verdad de cada monitor
        let frame_count = Arc::new(AtomicU32::new(0));
        for (_, mut rx) in receivers {
            let count = frame_count.clone();
            helpers.push(tokio::spawn(async move {
                while rx.changed().await.is_ok() {
                    if rx.borrow_and_update().is_some() {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        helpers.push(tokio::spawn(stats_loop(
            shared.clone(),
            frame_count,
            counters,
        )));

        shared.follow_devices();
        let session = Self {
            shared,
            devices: tasks,
            sources: started,
            helpers,
            options,
        };
        let mut session = session;
        if let Some(power) = session.options.power.clone() {
            let senders = session.senders();
            let timeout = session.options.suspend_timeout;
            let mut events = power.subscribe();
            session.helpers.push(tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(PowerEvent::Suspend) => suspend_all(&senders, timeout).await,
                        Ok(PowerEvent::Resume) => resume_all(&senders).await,
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
            }));
        }
        Ok(session)
    }

    /// Estado global de la sesión.
    pub fn state(&self) -> SessionState {
        self.shared.session_state()
    }

    /// Estado de un foco dentro de la sesión.
    pub fn device_state(&self, device: &DeviceId) -> Option<DeviceSessionState> {
        self.shared.device_state(device)
    }

    /// Eventos de la sesión (estado, salud, color en vivo, estadísticas).
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.shared.subscribe()
    }

    fn senders(&self) -> Vec<mpsc::Sender<Command>> {
        self.devices.iter().map(|d| d.commands.clone()).collect()
    }

    /// El sistema se va a dormir: devuelve cada foco a su estado y congela la sesión.
    /// Es lo mismo que hace al recibir [`PowerEvent::Suspend`].
    pub async fn suspend(&self) {
        suspend_all(&self.senders(), self.options.suspend_timeout).await;
    }

    /// El sistema despertó: retoma. Es lo mismo que hace al recibir [`PowerEvent::Resume`].
    pub async fn resume(&self) {
        resume_all(&self.senders()).await;
    }

    /// Detiene la sesión: devuelve cada foco a su estado original, termina las capturas y
    /// libera los focos. Espera como mucho `stop_timeout`.
    pub async fn stop(mut self) {
        self.shared.set_session(SessionState::Restoring);
        let mut waiting = Vec::new();
        for device in &self.devices {
            let (tx, rx) = oneshot::channel();
            if device.commands.send(Command::Stop(tx)).await.is_ok() {
                waiting.push((device.id.clone(), rx));
            }
        }
        // Un solo plazo para todos los focos: `stop_timeout` acota la parada completa
        let deadline = Instant::now() + self.options.stop_timeout;
        for (id, rx) in waiting {
            if tokio::time::timeout_at(deadline, rx).await.is_err() {
                tracing::warn!(device = %id, "el foco no terminó de restaurarse a tiempo");
            }
        }
        for device in self.devices.drain(..) {
            // Si ya terminó no pasa nada; si quedó colgada, se corta acá
            device.join.abort();
        }
        for helper in self.helpers.drain(..) {
            helper.abort();
        }
        for mut source in self.sources.drain(..) {
            source.stop();
        }
        self.shared.set_session(SessionState::Idle);
    }
}

impl Drop for Session {
    /// Descartar la sesión sin `stop()` corta las tareas sin restaurar los focos.
    fn drop(&mut self) {
        for device in &self.devices {
            device.join.abort();
        }
        for helper in &self.helpers {
            helper.abort();
        }
        for source in &mut self.sources {
            source.stop();
        }
    }
}

async fn suspend_all(senders: &[mpsc::Sender<Command>], timeout: Duration) {
    let mut waiting = Vec::new();
    for tx in senders {
        let (done, rx) = oneshot::channel();
        if tx.send(Command::Suspend(done)).await.is_ok() {
            waiting.push(rx);
        }
    }
    let wait_all = async {
        for rx in waiting {
            let _ = rx.await;
        }
    };
    if tokio::time::timeout(timeout, wait_all).await.is_err() {
        tracing::warn!("no todos los focos se restauraron antes de dormir");
    }
}

async fn resume_all(senders: &[mpsc::Sender<Command>]) {
    for tx in senders {
        let _ = tx.send(Command::Resume).await;
    }
}

/// Publica cada segundo los cuadros que llegaron y los envíos de cada foco.
async fn stats_loop(
    shared: Arc<Shared>,
    frames: Arc<AtomicU32>,
    devices: Vec<(DeviceId, Arc<AtomicU32>)>,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.tick().await; // el primero sale al instante
    let mut last = Instant::now();
    loop {
        ticker.tick().await;
        let elapsed = last.elapsed().as_secs_f32().max(f32::EPSILON);
        last = Instant::now();
        shared.emit(SessionEvent::Stats(PipelineStatsEvent {
            capture_fps: frames.swap(0, Ordering::Relaxed) as f32 / elapsed,
            devices: devices
                .iter()
                .map(|(id, sends)| DeviceRate {
                    device: id.clone(),
                    sends_per_s: sends.swap(0, Ordering::Relaxed) as f32 / elapsed,
                })
                .collect(),
        }));
    }
}
