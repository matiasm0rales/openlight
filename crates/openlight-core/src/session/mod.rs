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
//! - **Con la sesión andando** se pueden cambiar ajustes y zonas
//!   ([`Session::update_settings`]), sumar un foco ([`Session::add_device`]) o sacarlo
//!   ([`Session::remove_device`]) sin cortar a los demás.
//!
//! Siempre hay que terminar con [`Session::stop`]: es lo que espera a que los focos vuelvan a
//! su estado original.

mod runner;
mod shared;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use color_engine::Calibration;
use openlight_types::ipc::{DeviceRate, PipelineStatsEvent};
use openlight_types::{
    DeviceId, DeviceSessionState, DisplayId, Frame, ScreenSource, SessionState, Settings, Zone,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::driver::LightDriver;
use crate::engine::{DeviceEngine, Timing};
use crate::error::{CoreError, Result};
use crate::locks::{DeviceGuard, DeviceLocks, Holder};
use crate::pipeline::ColorPipeline;
use crate::power::{PowerEvent, PowerEvents};
use runner::{Command, DeviceRunner, Tuning, retry_every};
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

/// Zonas y ajustes nuevos de un foco que ya está en la sesión.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceTuning {
    /// El foco.
    pub id: DeviceId,
    /// Las zonas que sigue (al menos una).
    pub zones: Vec<Zone>,
    /// Sus ajustes efectivos.
    pub settings: Settings,
}

/// Cuánto se espera, al sacar un foco que no está andando, antes de cortar su tarea: si está
/// buscando un foco que no aparece, no tiene nada que restaurar.
const REMOVE_GRACE: Duration = Duration::from_secs(2);

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
    /// Zonas, ajustes y cuadros actuales; la tarea los toma cuando cambian.
    tuning: watch::Sender<Tuning>,
    join: JoinHandle<()>,
}

/// La captura de un monitor.
struct Capture {
    source: Box<dyn ScreenSource>,
    fps: f32,
    frames: watch::Receiver<Option<Frame>>,
    /// Cuenta los cuadros que llegan, para las estadísticas.
    counter: JoinHandle<()>,
}

impl Capture {
    fn stop(mut self) {
        self.counter.abort();
        self.source.stop();
    }
}

/// Lo que miran las tareas auxiliares (energía y estadísticas) de los focos que hay ahora.
#[derive(Default)]
struct Roster {
    devices: BTreeMap<DeviceId, Member>,
    /// El sistema está por dormir o dormido.
    suspended: bool,
}

struct Member {
    commands: mpsc::Sender<Command>,
    /// Envíos de color desde la última estadística.
    sends: Arc<AtomicU32>,
}

fn lock(roster: &Mutex<Roster>) -> MutexGuard<'_, Roster> {
    roster.lock().unwrap_or_else(PoisonError::into_inner)
}

fn senders(roster: &Mutex<Roster>) -> Vec<mpsc::Sender<Command>> {
    lock(roster)
        .devices
        .values()
        .map(|m| m.commands.clone())
        .collect()
}

/// Los cuadros por segundo que necesita cada monitor: los del foco más exigente que lo mira.
fn wanted_fps<'a>(
    devices: impl IntoIterator<Item = (&'a [Zone], &'a Settings)>,
) -> BTreeMap<DisplayId, f32> {
    let mut fps = BTreeMap::new();
    for (zones, settings) in devices {
        for zone in zones {
            let entry = fps.entry(zone.display.clone()).or_insert(0.0_f32);
            *entry = entry.max(settings.capture_fps);
        }
    }
    fps
}

fn check_zones(zones: &[Zone]) -> Result<()> {
    if zones.is_empty() {
        return Err(CoreError::NothingToDo);
    }
    match zones.iter().find(|z| !z.rect.is_valid()) {
        Some(zone) => Err(CoreError::Invalid(format!(
            "zona fuera del monitor {}: {:?}",
            zone.display, zone.rect
        ))),
        None => Ok(()),
    }
}

/// Una sesión en marcha.
pub struct Session {
    shared: Arc<Shared>,
    devices: Vec<DeviceTask>,
    captures: BTreeMap<DisplayId, Capture>,
    sources: SourceFactory,
    locks: DeviceLocks,
    roster: Arc<Mutex<Roster>>,
    /// Cuadros que llegaron desde la última estadística.
    frame_count: Arc<AtomicU32>,
    /// Estadísticas y escucha de energía.
    helpers: Vec<JoinHandle<()>>,
    options: SessionOptions,
    started_at: Instant,
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
        for device in &devices {
            check_zones(&device.zones)?;
        }

        // Si algo falla de acá en adelante, soltar los candados (los guardas) es suficiente
        let mut guards: BTreeMap<DeviceId, DeviceGuard> = BTreeMap::new();
        for device in &devices {
            let guard = locks.try_acquire(&device.id, Holder::Session)?;
            guards.insert(device.id.clone(), guard);
        }

        let shared = Arc::new(Shared::new(devices.iter().map(|d| d.id.clone())));
        shared.set_session(SessionState::Connecting);

        let mut session = Self {
            shared,
            devices: Vec::new(),
            captures: BTreeMap::new(),
            sources,
            locks: locks.clone(),
            roster: Arc::default(),
            frame_count: Arc::default(),
            helpers: Vec::new(),
            options,
            started_at: Instant::now(),
        };
        // Una captura por monitor, a los cuadros por segundo del foco más exigente
        let fps = wanted_fps(devices.iter().map(|d| (d.zones.as_slice(), &d.settings)));
        match session.start_captures(&fps) {
            Ok(captures) => session.captures = captures,
            Err(error) => {
                session.shared.set_session(SessionState::Idle);
                return Err(error);
            }
        }
        for device in devices {
            let guard = guards.remove(&device.id).expect("candado tomado arriba");
            session.spawn_device(device, guard);
        }
        session.helpers.push(tokio::spawn(stats_loop(
            session.shared.clone(),
            session.frame_count.clone(),
            session.roster.clone(),
        )));

        session.shared.follow_devices();
        if let Some(power) = session.options.power.clone() {
            let roster = session.roster.clone();
            let timeout = session.options.suspend_timeout;
            let mut events = power.subscribe();
            session.helpers.push(tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(PowerEvent::Suspend) => suspend_all(&roster, timeout).await,
                        Ok(PowerEvent::Resume) => resume_all(&roster).await,
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
            }));
        }
        Ok(session)
    }

    /// Empieza las capturas de `wanted` que todavía no están andando a esos cuadros por segundo.
    /// No toca las actuales; si una falla, detiene las que había empezado.
    fn start_captures(
        &self,
        wanted: &BTreeMap<DisplayId, f32>,
    ) -> Result<BTreeMap<DisplayId, Capture>> {
        let mut started = BTreeMap::new();
        for (display, &fps) in wanted {
            if self.captures.get(display).is_some_and(|c| c.fps == fps) {
                continue;
            }
            let mut source = (self.sources)();
            match source.start(display.clone(), self.options.capture_size, fps) {
                Ok(frames) => {
                    let count = self.frame_count.clone();
                    let mut rx = frames.clone();
                    let counter = tokio::spawn(async move {
                        while rx.changed().await.is_ok() {
                            if rx.borrow_and_update().is_some() {
                                count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                    started.insert(
                        display.clone(),
                        Capture {
                            source,
                            fps,
                            frames,
                            counter,
                        },
                    );
                }
                Err(error) => {
                    started.into_values().for_each(Capture::stop);
                    return Err(error.into());
                }
            }
        }
        Ok(started)
    }

    /// Suma las capturas nuevas y detiene las reemplazadas y las que ya nadie mira.
    fn install_captures(
        &mut self,
        started: BTreeMap<DisplayId, Capture>,
        wanted: &BTreeMap<DisplayId, f32>,
    ) {
        for (display, capture) in started {
            if let Some(old) = self.captures.insert(display, capture) {
                old.stop();
            }
        }
        let unused: Vec<DisplayId> = self
            .captures
            .keys()
            .filter(|d| !wanted.contains_key(d))
            .cloned()
            .collect();
        for display in unused {
            if let Some(capture) = self.captures.remove(&display) {
                capture.stop();
            }
        }
    }

    /// Los cuadros de cada monitor que usan `zones`.
    fn receivers_for(&self, zones: &[Zone]) -> HashMap<DisplayId, watch::Receiver<Option<Frame>>> {
        zones
            .iter()
            .map(|z| z.display.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|d| self.captures.get(&d).map(|c| (d.clone(), c.frames.clone())))
            .collect()
    }

    /// Le avisa a cada foco al que le cambió algo: zonas, ajustes o la captura de un monitor.
    fn retune_devices(&self, changes: &BTreeMap<DeviceId, (Vec<Zone>, Settings)>) {
        for task in &self.devices {
            let current = task.tuning.borrow().clone();
            let (zones, settings) = changes
                .get(&task.id)
                .cloned()
                .unwrap_or_else(|| (current.zones.clone(), current.settings.clone()));
            let receivers = self.receivers_for(&zones);
            let same_frames = receivers.len() == current.receivers.len()
                && receivers.iter().all(|(display, rx)| {
                    current
                        .receivers
                        .get(display)
                        .is_some_and(|old| old.same_channel(rx))
                });
            if zones != current.zones || settings != current.settings || !same_frames {
                task.tuning.send_replace(Tuning {
                    zones,
                    settings,
                    receivers,
                });
            }
        }
    }

    /// Lanza la tarea de un foco, con sus capturas ya andando.
    fn spawn_device(&mut self, device: DeviceSpec, guard: DeviceGuard) {
        let sends = Arc::new(AtomicU32::new(0));
        let timing = Timing::from_settings(&device.settings);
        let initial = Tuning {
            receivers: self.receivers_for(&device.zones),
            zones: device.zones.clone(),
            settings: device.settings.clone(),
        };
        let pipeline = ColorPipeline::new(device.zones, &device.settings, device.calibration);
        let runner = DeviceRunner {
            id: device.id.clone(),
            engine: DeviceEngine::new(pipeline, timing),
            driver: device.driver,
            receivers: initial.receivers.clone(),
            shared: self.shared.clone(),
            started: self.started_at,
            retry_every: retry_every(&device.settings),
            sends: sends.clone(),
            _guard: guard,
            state: DeviceSessionState::Idle,
            last_live: None,
            last_health: None,
            retry_at: None,
        };
        let (commands, rx) = mpsc::channel(8);
        let (tuning, tuning_rx) = watch::channel(initial);
        {
            let mut roster = lock(&self.roster);
            if roster.suspended {
                // Entró con el sistema durmiéndose: que no se quede con el color
                let (done, _) = oneshot::channel();
                let _ = commands.try_send(Command::Suspend(done));
            }
            roster.devices.insert(
                device.id.clone(),
                Member {
                    commands: commands.clone(),
                    sends,
                },
            );
        }
        self.devices.push(DeviceTask {
            id: device.id,
            commands,
            tuning,
            join: tokio::spawn(runner.run(rx, tuning_rx)),
        });
    }

    /// Cambia zonas y ajustes de focos que ya están en la sesión, sin soltarlos. Casi todo vale
    /// al instante; si cambian los monitores usados o los cuadros por segundo, se rearman las
    /// capturas (ver [`openlight_types::SettingId::apply`]). Un foco que no está en la sesión se
    /// ignora.
    ///
    /// Falla sin cambiar nada si una zona es inválida o no se puede capturar un monitor.
    pub fn update_settings(&mut self, changes: Vec<DeviceTuning>) -> Result<()> {
        let mut updates = BTreeMap::new();
        for change in changes {
            if self.devices.iter().any(|d| d.id == change.id) {
                check_zones(&change.zones)?;
                updates.insert(change.id, (change.zones, change.settings));
            }
        }
        let next: Vec<(Vec<Zone>, Settings)> = self
            .devices
            .iter()
            .map(|task| {
                updates.get(&task.id).cloned().unwrap_or_else(|| {
                    let current = task.tuning.borrow();
                    (current.zones.clone(), current.settings.clone())
                })
            })
            .collect();
        let wanted = wanted_fps(next.iter().map(|(z, s)| (z.as_slice(), s)));
        let started = self.start_captures(&wanted)?;
        self.install_captures(started, &wanted);
        self.retune_devices(&updates);
        Ok(())
    }

    /// Suma un foco a la sesión en marcha. Falla sin cambiar nada si el foco está tomado, si no
    /// tiene zonas válidas o si no se puede capturar su monitor.
    pub fn add_device(&mut self, device: DeviceSpec) -> Result<()> {
        check_zones(&device.zones)?;
        let guard = self.locks.try_acquire(&device.id, Holder::Session)?;
        let current: Vec<(Vec<Zone>, Settings)> = self
            .devices
            .iter()
            .map(|task| {
                let current = task.tuning.borrow();
                (current.zones.clone(), current.settings.clone())
            })
            .collect();
        let wanted = wanted_fps(
            current
                .iter()
                .map(|(z, s)| (z.as_slice(), s))
                .chain([(device.zones.as_slice(), &device.settings)]),
        );
        let started = self.start_captures(&wanted)?;
        self.install_captures(started, &wanted);
        self.retune_devices(&BTreeMap::new());
        self.spawn_device(device, guard);
        Ok(())
    }

    /// Saca un foco de la sesión: lo devuelve a su estado, suelta su candado y detiene las
    /// capturas que quedaron sin uso. Los demás siguen. `false` si no estaba.
    ///
    /// Si el foco no está andando (buscándolo, reconectando) no se espera el plazo completo.
    pub async fn remove_device(&mut self, device: &DeviceId) -> bool {
        let Some(index) = self.devices.iter().position(|d| &d.id == device) else {
            return false;
        };
        let mut task = self.devices.remove(index);
        lock(&self.roster).devices.remove(device);
        let wait = match self.shared.device_state(device) {
            Some(
                DeviceSessionState::Active
                | DeviceSessionState::Paused
                | DeviceSessionState::Restoring,
            ) => self.options.stop_timeout,
            _ => REMOVE_GRACE.min(self.options.stop_timeout),
        };
        let (done, finished) = oneshot::channel();
        let stop = async {
            if task.commands.send(Command::Stop(done)).await.is_ok() {
                let _ = finished.await;
            }
        };
        if tokio::time::timeout(wait, stop).await.is_err() {
            tracing::warn!(device = %device, "el foco no terminó de restaurarse a tiempo");
        }
        task.join.abort();
        // Hasta que la tarea no termina, el candado sigue tomado
        let _ = (&mut task.join).await;
        self.shared.forget_device(device);

        let current: Vec<(Vec<Zone>, Settings)> = self
            .devices
            .iter()
            .map(|task| {
                let current = task.tuning.borrow();
                (current.zones.clone(), current.settings.clone())
            })
            .collect();
        let wanted = wanted_fps(current.iter().map(|(z, s)| (z.as_slice(), s)));
        // Sin el foco sacado, un monitor puede necesitar menos cuadros: se deja la captura como
        // está y solo se detienen las que nadie mira
        self.install_captures(BTreeMap::new(), &wanted);
        true
    }

    /// Los focos que están en la sesión.
    pub fn devices(&self) -> Vec<DeviceId> {
        self.devices.iter().map(|d| d.id.clone()).collect()
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

    /// El sistema se va a dormir: devuelve cada foco a su estado y congela la sesión.
    /// Es lo mismo que hace al recibir [`PowerEvent::Suspend`].
    pub async fn suspend(&self) {
        suspend_all(&self.roster, self.options.suspend_timeout).await;
    }

    /// El sistema despertó: retoma. Es lo mismo que hace al recibir [`PowerEvent::Resume`].
    pub async fn resume(&self) {
        resume_all(&self.roster).await;
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
        std::mem::take(&mut self.captures)
            .into_values()
            .for_each(Capture::stop);
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
        std::mem::take(&mut self.captures)
            .into_values()
            .for_each(Capture::stop);
    }
}

async fn suspend_all(roster: &Mutex<Roster>, timeout: Duration) {
    lock(roster).suspended = true;
    let mut waiting = Vec::new();
    for tx in senders(roster) {
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

async fn resume_all(roster: &Mutex<Roster>) {
    lock(roster).suspended = false;
    for tx in senders(roster) {
        let _ = tx.send(Command::Resume).await;
    }
}

/// Publica cada segundo los cuadros que llegaron y los envíos de cada foco.
async fn stats_loop(shared: Arc<Shared>, frames: Arc<AtomicU32>, roster: Arc<Mutex<Roster>>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.tick().await; // el primero sale al instante
    let mut last = Instant::now();
    loop {
        ticker.tick().await;
        let elapsed = last.elapsed().as_secs_f32().max(f32::EPSILON);
        last = Instant::now();
        shared.emit(SessionEvent::Stats(PipelineStatsEvent {
            capture_fps: frames.swap(0, Ordering::Relaxed) as f32 / elapsed,
            devices: lock(&roster)
                .devices
                .iter()
                .map(|(id, member)| DeviceRate {
                    device: id.clone(),
                    sends_per_s: member.sends.swap(0, Ordering::Relaxed) as f32 / elapsed,
                })
                .collect(),
        }));
    }
}
