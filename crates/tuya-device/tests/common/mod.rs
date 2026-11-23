//! Banco de pruebas: un foco simulado (`tuya-sim`) y un reloj de tokio pausado.
//!
//! Con el tiempo pausado, tokio avanza el reloj solo cuando no hay nada que hacer, aunque haya
//! bytes en camino por el socket: un timeout puede vencer justo antes de que llegue la
//! respuesta. Por eso las pruebas **manejan el reloj a mano**: mientras `drive` o `pump` corren,
//! la runtime nunca está ociosa (siempre hay una tarea cediendo el turno), así que el reloj solo
//! se mueve cuando la prueba lo avanza, y entre paso y paso el actor y el simulador tienen
//! tiempo de terminar lo que empezaron. La runtime se arma con `event_interval(1)` para que el
//! IO se atienda en cada vuelta del scheduler.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use openlight_types::{DeviceCreds, DeviceId, Dps, ProtocolVersion, Secret};
use serde_json::{Value, json};
use tokio::runtime::{Builder, Runtime};
use tokio::time::Instant;
use tuya_device::{DeviceConfig, DeviceHandle, IpMode, Locator, MemoryStore, StateStore, spawn};
use tuya_sim::{DEFAULT_LOCAL_KEY, Faults, SimConfig, SimHandle};

/// Cada cuánto avanza el reloj en `pump` y `pump_until`.
pub const STEP: Duration = Duration::from_millis(10);

/// Cada cuánto avanza el reloj en `drive` (esperas largas donde no importa la precisión).
pub const DRIVE_STEP: Duration = Duration::from_millis(50);

pub const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Una runtime con el reloj pausado y el IO atendido en cada vuelta.
pub fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .event_interval(1)
        .build()
        .expect("runtime de pruebas")
}

/// Corre una prueba async con el reloj pausado.
pub fn run<F: Future>(test: F) -> F::Output {
    runtime().block_on(test)
}

/// Cede el turno varias veces para que los demás terminen lo que tienen pendiente.
pub async fn settle() {
    settle_n(32).await;
}

/// Como [`settle`] con otra cantidad de vueltas (menos es más rápido, pero más justo).
pub async fn settle_n(turns: u32) {
    for _ in 0..turns {
        tokio::task::yield_now().await;
    }
}

/// Deja pasar `total` de tiempo simulado, de a [`STEP`], dejando trabajar a los demás.
pub async fn pump(total: Duration) {
    pump_by(total, STEP).await;
}

/// Como [`pump`] pero de a [`DRIVE_STEP`]: para esperas largas donde no importa la precisión.
pub async fn pump_slow(total: Duration) {
    pump_by(total, DRIVE_STEP).await;
}

async fn pump_by(total: Duration, step: Duration) {
    let mut left = total;
    while !left.is_zero() {
        let step = left.min(step);
        settle().await;
        tokio::time::advance(step).await;
        left -= step;
    }
    settle().await;
}

/// Espera el resultado de `fut` avanzando el reloj de a [`DRIVE_STEP`] solo cuando todos están quietos.
pub async fn drive<F: Future>(fut: F) -> F::Output {
    tokio::pin!(fut);
    loop {
        tokio::select! {
            biased;
            out = &mut fut => return out,
            () = settle() => {}
        }
        tokio::time::advance(DRIVE_STEP).await;
    }
}

/// Avanza el reloj hasta que `cond` se cumpla (o falla si pasan `limit`).
pub async fn pump_until(limit: Duration, what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < limit, "no se cumplió: {what}");
        pump(STEP).await;
    }
}

/// Un [`Locator`] que contesta lo que se le diga y avisa cuántas veces lo llamaron.
#[derive(Default)]
pub struct MockLocator {
    answer: Mutex<Option<IpAddr>>,
    calls: AtomicU32,
    hook: Mutex<Option<Box<dyn FnMut() + Send>>>,
}

impl MockLocator {
    pub fn new(answer: Option<IpAddr>) -> Arc<Self> {
        Arc::new(Self {
            answer: Mutex::new(answer),
            ..Self::default()
        })
    }

    pub fn answer(&self, ip: Option<IpAddr>) {
        *self.answer.lock().unwrap() = ip;
    }

    /// Algo que correr cada vez que preguntan (por ejemplo, arreglar al foco).
    pub fn on_locate(&self, hook: impl FnMut() + Send + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }

    pub fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Locator for MockLocator {
    async fn locate(&self, _id: &DeviceId) -> Option<IpAddr> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = self.hook.lock().unwrap().as_mut() {
            hook();
        }
        *self.answer.lock().unwrap()
    }
}

/// Un foco simulado por TCP en un puerto efímero.
pub async fn sim(version: ProtocolVersion) -> SimHandle {
    sim_with(version, Faults::default()).await
}

pub async fn sim_with(version: ProtocolVersion, faults: Faults) -> SimHandle {
    tuya_sim::start(SimConfig::new(version).ephemeral().faults(faults))
        .await
        .expect("arranca el simulador")
}

/// Las capacidades que saldrían del mapeo de la nube de este foco.
pub fn caps_of(sim: &SimHandle) -> openlight_types::Capabilities {
    use tuya_device::CapabilitiesExt;
    openlight_types::Capabilities::from_mapping(sim.mapping()).expect("mapeo de una luz")
}

/// Un puerto donde no escucha nadie (se reserva y se suelta).
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Configuración para hablar con `sim` por `127.0.0.1`.
pub fn config_for(sim: &SimHandle) -> DeviceConfig {
    config_with(sim.dev_id().clone(), sim.version(), sim.port())
}

pub fn config_with(id: DeviceId, version: ProtocolVersion, port: u16) -> DeviceConfig {
    DeviceConfig::new(DeviceCreds {
        id,
        local_key: Secret::new(DEFAULT_LOCAL_KEY.to_owned()),
        version,
        ip: Some(LOCALHOST),
    })
    .ip_mode(IpMode::Auto)
    .port(port)
}

/// Todo lo de una prueba: el foco simulado, el almacén, el locator y el actor.
pub struct Rig {
    pub sim: Arc<SimHandle>,
    pub store: Arc<MemoryStore>,
    pub locator: Arc<MockLocator>,
    pub device: DeviceHandle,
    pub id: DeviceId,
}

impl Rig {
    pub async fn new(version: ProtocolVersion) -> Self {
        Self::with(sim(version).await, |c| c)
    }

    /// Con un simulador ya armado y un ajuste de la configuración.
    pub fn with(sim: SimHandle, tweak: impl FnOnce(DeviceConfig) -> DeviceConfig) -> Self {
        let config = tweak(config_for(&sim));
        let id = config.creds.id.clone();
        let store = Arc::new(MemoryStore::new());
        let locator = MockLocator::new(Some(LOCALHOST));
        let store_dyn: Arc<dyn StateStore> = store.clone();
        let device = spawn(config, locator.clone(), store_dyn);
        Self {
            sim: Arc::new(sim),
            store,
            locator,
            device,
            id,
        }
    }

    /// `start()` avanzando el reloj.
    pub async fn start(&self) -> tuya_device::StartInfo {
        drive(self.device.start()).await.expect("start")
    }

    /// Activa el foco, deja que el simulador reciba el modo color y pone en cero lo que contó.
    pub async fn activate(&self) {
        self.device.activate().await.expect("activate");
        pump(STEP).await;
        self.sim.reset_stats();
    }

    /// Escrituras que recibió el foco, juntas por DP (último valor de cada uno, en orden).
    pub fn writes(&self) -> Vec<Dps> {
        self.sim.writes()
    }
}

/// `ESTADO_BLANCO` de `test_ambilight.py`: el foco como estaba antes de arrancar.
pub fn estado_blanco() -> Dps {
    dps(json!({"20": true, "21": "white", "22": 500, "23": 100, "24": "000000000000"}))
}

pub fn dps(value: Value) -> Dps {
    let map: BTreeMap<String, Value> = serde_json::from_value(value).expect("DPs de prueba");
    map
}

pub fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(LOCALHOST, port)
}
