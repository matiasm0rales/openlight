//! Arnés de las pruebas de integración: el backend de verdad con todo lo de afuera falso.
//!
//! - llavero en memoria (`MemoryStore`): **nunca** el real;
//! - carpeta de datos temporal;
//! - focos: uno o más `tuya-sim` en loopback, con clave falsa;
//! - nube falsa que devuelve esos focos;
//! - captura falsa con un "video" sintético cuyo color se cambia desde la prueba;
//! - eventos a un `MemorySink`.
#![allow(dead_code, missing_docs)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use openlight_lib::bridge::lan::{FixedScanner, LanScanner};
use openlight_lib::events::{AppEvent, MemorySink};
use openlight_lib::state::{Backend, BackendOptions, BackendParts};
use openlight_types::ipc::DiscoveredDevice;
use openlight_types::{
    CaptureError, CloudAccount, CloudCredentials, CloudDevice, CloudError, DeviceId, DisplayId,
    DisplayInfo, DpMapping, Frame, PermissionState, ProtocolVersion, Region, ScreenSource, Secret,
    TuyaCloud,
};
use secrets::MemoryStore;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tuya_device::NoLocator;
use tuya_sim::{SimConfig, SimHandle};

/// Access ID y Access Secret falsos que acepta la nube falsa.
pub const FAKE_ACCESS_ID: &str = "fake-access-id";
pub const FAKE_ACCESS_SECRET: &str = "fake-access-secret";

/// El monitor falso.
pub const DISPLAY: &str = "fake-display-1";

/// Color del "video" sintético (BGRA, sin transparencia) y las capturas que se pidieron.
#[derive(Clone, Default)]
pub struct Screen {
    rgb: Arc<Mutex<[u8; 3]>>,
    starts: Arc<Mutex<Vec<f32>>>,
}

impl Screen {
    /// Pone toda la pantalla de un color RGB.
    pub fn paint(&self, rgb: [u8; 3]) {
        *self.rgb.lock().unwrap_or_else(PoisonError::into_inner) = rgb;
    }

    fn rgb(&self) -> [u8; 3] {
        *self.rgb.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Los cuadros por segundo de cada captura que se arrancó, en orden.
    pub fn capture_starts(&self) -> Vec<f32> {
        self.starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Una captura falsa: publica cuadros del color de [`Screen`] a 20 por segundo.
struct FakeSource {
    screen: Screen,
    task: Option<JoinHandle<()>>,
}

impl ScreenSource for FakeSource {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(vec![DisplayInfo {
            id: DisplayId(DISPLAY.into()),
            name: "Monitor falso".into(),
            width: 1920,
            height: 1080,
            builtin: false,
            primary: true,
        }])
    }

    fn start(
        &mut self,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
        if display.0 != DISPLAY {
            return Err(CaptureError::DisplayNotFound(display));
        }
        self.screen
            .starts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fps);
        let (tx, rx) = watch::channel(None);
        let screen = self.screen.clone();
        let period = Duration::from_secs_f32(1.0 / fps.max(1.0));
        self.task = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            loop {
                ticker.tick().await;
                let [r, g, b] = screen.rgb();
                let pixels = (target.0 * target.1) as usize;
                let bgra: Vec<u8> = std::iter::repeat_n([b, g, r, 255], pixels)
                    .flatten()
                    .collect();
                let frame = Frame::new(target.0, target.1, bgra).expect("cuadro válido");
                if tx.send(Some(frame)).is_err() {
                    return;
                }
            }
        }));
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }

    fn permission(&self) -> PermissionState {
        PermissionState::Granted
    }

    fn request_permission(&self) -> PermissionState {
        PermissionState::Granted
    }
}

/// Una nube falsa con los focos que se le den.
pub struct FakeCloud {
    pub devices: Vec<CloudDevice>,
    pub mapping: DpMapping,
    pub region: Region,
}

#[async_trait]
impl TuyaCloud for FakeCloud {
    async fn validate(&self) -> Result<CloudAccount, CloudError> {
        Ok(CloudAccount {
            uid: "fake-uid".into(),
            region: self.region,
        })
    }

    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError> {
        Ok(self.devices.clone())
    }

    async fn mapping(&self, _device: &DeviceId) -> Result<DpMapping, CloudError> {
        Ok(self.mapping.clone())
    }
}

/// Qué ve la búsqueda en la red.
#[derive(Clone)]
pub enum ScanMode {
    /// Estos focos.
    Found(Vec<DiscoveredDevice>),
    /// Nada: la red está vacía.
    Empty,
    /// No se puede ni escuchar (como un puerto ocupado por otro programa).
    Fails,
}

/// Un escáner que se cambia en caliente desde la prueba.
#[derive(Clone)]
pub struct Scan(Arc<Mutex<ScanMode>>);

impl Scan {
    pub fn set(&self, mode: ScanMode) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = mode;
    }
}

#[async_trait]
impl LanScanner for Scan {
    async fn scan(
        &self,
        window: Duration,
        until: Option<&DeviceId>,
        found: &(dyn Fn(DiscoveredDevice) + Send + Sync),
    ) -> Result<Vec<DiscoveredDevice>, openlight_types::UserFacingError> {
        let mode = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match mode {
            ScanMode::Found(devices) => FixedScanner(devices).scan(window, until, found).await,
            ScanMode::Empty => Ok(Vec::new()),
            ScanMode::Fails => Err(openlight_types::UserFacingError::new(
                openlight_types::ErrorCode::FirewallBlocked,
            )
            .with_param("detail", "puerto UDP en uso")
            .with_action(openlight_types::FixAction::Retry)),
        }
    }
}

/// Todo lo armado.
pub struct Harness {
    pub scan: Scan,
    pub cloud_factory: openlight_lib::state::CloudFactory,
    pub backend: Arc<Backend>,
    pub events: MemorySink,
    pub secrets: Arc<MemoryStore>,
    pub screen: Screen,
    pub dir: tempfile::TempDir,
    pub sims: Vec<SimHandle>,
}

/// Opciones del arnés.
pub struct Setup {
    /// Cuántos focos simulados (ids `openlightsimdevice0001`, `...0002`).
    pub sims: usize,
    /// Los focos aparecen en la búsqueda de la red.
    pub lan: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self { sims: 1, lan: true }
    }
}

/// El id del foco simulado número `n` (desde 1).
pub fn sim_id(n: usize) -> DeviceId {
    DeviceId(format!("openlightsimdevice{n:04}"))
}

pub async fn harness(setup: Setup) -> Harness {
    harness_with(setup, |_| {}).await
}

/// Como [`harness`], retocando los tiempos del backend.
pub async fn harness_with(setup: Setup, tune: impl FnOnce(&mut BackendOptions)) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let events = MemorySink::new();
    let secrets = Arc::new(MemoryStore::new());
    let screen = Screen::default();

    let mut sims = Vec::new();
    let mut cloud_devices = Vec::new();
    let mut lan = Vec::new();
    let mut ports: HashMap<DeviceId, u16> = HashMap::new();
    for n in 1..=setup.sims {
        let sim = tuya_sim::start(
            SimConfig::new(ProtocolVersion::V35)
                .ephemeral()
                .dev_id(sim_id(n).0),
        )
        .await
        .unwrap();
        ports.insert(sim_id(n), sim.port());
        cloud_devices.push(CloudDevice {
            id: sim_id(n),
            name: format!("Foco falso {n}"),
            local_key: Secret::new(String::from_utf8(sim.local_key().to_vec()).unwrap()),
            product_id: "fakeproduct".into(),
            category: "dj".into(),
            model: None,
            online: true,
            cloud_ip: None,
        });
        lan.push(DiscoveredDevice {
            id: sim_id(n),
            ip: "127.0.0.1".into(),
            version: ProtocolVersion::V35,
            product_key: None,
        });
        sims.push(sim);
    }
    let mapping = sims
        .first()
        .map(|s| s.mapping().clone())
        .unwrap_or_default();

    let tweak_ports = ports;
    let mut options = BackendOptions {
        test_step: Duration::from_millis(80),
        manual_idle: Duration::from_millis(400),
        list_scan: Duration::from_millis(50),
        lookup_scan: Duration::from_millis(50),
        device_tweak: Some(Arc::new(move |id, config| match tweak_ports.get(id) {
            Some(port) => config.port(*port),
            None => config,
        })),
        ..BackendOptions::default()
    };
    tune(&mut options);
    let scan = Scan(Arc::new(Mutex::new(if setup.lan {
        ScanMode::Found(lan.clone())
    } else {
        ScanMode::Empty
    })));
    let cloud_screen = screen.clone();
    let cloud_factory: openlight_lib::state::CloudFactory =
        Arc::new(move |creds: CloudCredentials| {
            if creds.access_id != FAKE_ACCESS_ID
                || creds.access_secret.expose() != FAKE_ACCESS_SECRET
            {
                // Un cliente que falla al validar, como la nube con un secreto mal copiado
                return Ok(Arc::new(RejectingCloud) as Arc<dyn TuyaCloud>);
            }
            Ok(Arc::new(FakeCloud {
                devices: cloud_devices.clone(),
                mapping: mapping.clone(),
                region: creds.region,
            }) as Arc<dyn TuyaCloud>)
        });
    let backend = Backend::new(BackendParts {
        data_dir: dir.path().join("datos"),
        secrets: secrets.clone(),
        events: Arc::new(events.clone()),
        sources: Arc::new(move || {
            Box::new(FakeSource {
                screen: cloud_screen.clone(),
                task: None,
            })
        }),
        cloud: cloud_factory.clone(),
        scanner: Arc::new(scan.clone()),
        locator: Arc::new(NoLocator),
        options,
    })
    .unwrap();

    Harness {
        scan,
        cloud_factory,
        backend,
        events,
        secrets,
        screen,
        dir,
        sims,
    }
}

/// Una nube que rechaza todo con "firma inválida".
struct RejectingCloud;

#[async_trait]
impl TuyaCloud for RejectingCloud {
    async fn validate(&self) -> Result<CloudAccount, CloudError> {
        Err(CloudError::InvalidSignature)
    }

    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError> {
        Err(CloudError::InvalidSignature)
    }

    async fn mapping(&self, _device: &DeviceId) -> Result<DpMapping, CloudError> {
        Err(CloudError::InvalidSignature)
    }
}

/// Otro backend sobre la misma carpeta de datos y el mismo llavero: como reabrir la app.
pub fn reopen(h: &Harness) -> Arc<Backend> {
    Backend::new(BackendParts {
        data_dir: h.backend.data_dir().to_path_buf(),
        secrets: h.secrets.clone(),
        events: Arc::new(MemorySink::new()),
        sources: Arc::new(|| unreachable!("reopen no captura")),
        cloud: Arc::new(|_| Err(CloudError::Network("sin red".into()))),
        scanner: Arc::new(FixedScanner(Vec::new())),
        locator: Arc::new(NoLocator),
        options: BackendOptions::default(),
    })
    .unwrap()
}

/// Como [`reopen`] pero con el mismo escáner, la misma nube y las mismas credenciales: la app
/// reabierta, con la caché de la red vacía.
pub fn reopen_with(h: &Harness) -> Arc<Backend> {
    Backend::new(BackendParts {
        data_dir: h.backend.data_dir().to_path_buf(),
        secrets: h.secrets.clone(),
        events: Arc::new(MemorySink::new()),
        sources: Arc::new(|| unreachable!("reopen no captura")),
        cloud: h.cloud_factory.clone(),
        scanner: Arc::new(h.scan.clone()),
        locator: Arc::new(NoLocator),
        options: BackendOptions::default(),
    })
    .unwrap()
}

impl Harness {
    /// Eventos emitidos hasta ahora que cumplen `pick`.
    pub fn events_where<T>(&self, pick: impl Fn(&AppEvent) -> Option<T>) -> Vec<T> {
        self.events.events().iter().filter_map(pick).collect()
    }
}

/// Espera hasta que `cond` se cumpla (con tiempo real), o falla con `what`.
pub async fn eventually(what: &str, limit: Duration, mut cond: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !cond() {
        assert!(start.elapsed() < limit, "no se cumplió a tiempo: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
