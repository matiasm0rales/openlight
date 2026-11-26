//! Banco de rendimiento (criterio de `docs/PLAN.md` 4.3): la app de verdad con todo lo de afuera
//! falso. Dos focos `tuya-sim` en un proceso aparte (así su CPU no cuenta), una captura
//! sintética de 96×54 que recorre el mismo camino que la de macOS, llavero en memoria y carpeta
//! de datos temporal. **No toca focos, red, pantalla ni llavero reales.**
//!
//! ```text
//! # Solo Rust: CPU, asignaciones y eventos del pipeline por segundo
//! cargo run --release -p openlight --example banco -- pipeline [--segundos 30] [--escena movil|fija]
//!
//! # Con Tauri y la UI de verdad en una ventana oculta: CPU y memoria del proceso y del WebContent
//! pnpm --filter openlight-ui build
//! cargo run --release -p openlight --features tauri/custom-protocol --example banco -- ventana [--segundos 60]
//!
//! # Además, muestra la ventana (fuera de la pantalla y sin sacar el foco) y la cierra, para ver
//! # que los eventos de dibujo vuelven y se cortan
//! ... banco ventana --segundos 10 --ciclo si
//! ```
//!
//! La CPU está en % de un núcleo (como `top`) y la memoria es la columna `MEM` de `top`. Las
//! mediciones usan `top` y por eso solo salen en macOS.

// Contar asignaciones pide un `GlobalAlloc`, y saber el PID del WebContent, un mensaje a
// WKWebView. Es una herramienta de medición, no entra en la app.
#![allow(unsafe_code, missing_docs)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use openlight_lib::bridge::lan::FixedScanner;
use openlight_lib::events::{AppEvent, EventSink};
use openlight_lib::state::{Backend, BackendOptions, BackendParts};
use openlight_types::ipc::{
    CloudCredentialsInput, DeviceAddRequest, DiscoveredDevice, OnboardingStep,
};
use openlight_types::{
    CaptureError, CloudAccount, CloudCredentials, CloudDevice, CloudError, DeviceId, DisplayId,
    DisplayInfo, DpMapping, Frame, PermissionState, ProtocolVersion, Region, ScreenSource, Secret,
    TuyaCloud, Zone, ZonePreset,
};
use secrets::{MemoryStore, SecretStore};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tuya_device::NoLocator;
use tuya_sim::SimConfig;

const FOCOS: usize = 2;
const MONITOR: &str = "banco-1";
/// Cuánto se deja andar la sesión antes de empezar a medir.
const CALENTAMIENTO: Duration = Duration::from_secs(5);

// ---------- Asignaciones ----------

struct Contador;

static ASIGNACIONES: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: delega todo en el asignador del sistema; solo suma contadores atómicos.
unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ASIGNACIONES.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: mismo contrato que `System::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ASIGNACIONES.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: mismo contrato que `System::alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ASIGNACIONES.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        // SAFETY: mismo contrato que `System::realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: mismo contrato que `System::dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ASIGNADOR: Contador = Contador;

// ---------- Focos simulados (proceso aparte) ----------

/// El proceso hijo: levanta los focos, dice dónde escuchan y contesta cuántas escrituras
/// recibieron cada vez que le mandan una línea. Termina cuando se le cierra la entrada.
fn proceso_sim() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime");
    let _guard = runtime.enter();
    let sims: Vec<_> = (1..=FOCOS)
        .map(|n| {
            let config = SimConfig::new(ProtocolVersion::V35)
                .ephemeral()
                .dev_id(id_foco(n).0);
            runtime.block_on(tuya_sim::start(config)).expect("tuya-sim")
        })
        .collect();
    let focos: Vec<Value> = sims
        .iter()
        .map(|s| {
            json!({
                "port": s.port(),
                "key": String::from_utf8_lossy(&s.local_key()),
                "mapping": s.mapping(),
            })
        })
        .collect();
    println!("{}", json!(focos));
    for linea in std::io::stdin().lock().lines() {
        if linea.is_err() {
            break;
        }
        let escrituras: Vec<usize> = sims.iter().map(|s| s.writes().len()).collect();
        println!("{}", json!(escrituras));
        let _ = std::io::stdout().flush();
    }
}

struct Focos {
    proceso: Child,
    entrada: ChildStdin,
    salida: BufReader<ChildStdout>,
    datos: Vec<(u16, String, DpMapping)>,
}

impl Focos {
    fn lanzar() -> Self {
        let yo = std::env::current_exe().expect("ruta del ejecutable");
        let mut proceso = Command::new(yo)
            .arg("sim")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("no arrancó el proceso de los focos");
        let entrada = proceso.stdin.take().expect("stdin");
        let mut salida = BufReader::new(proceso.stdout.take().expect("stdout"));
        let mut linea = String::new();
        salida
            .read_line(&mut linea)
            .expect("sin datos de los focos");
        let datos = serde_json::from_str::<Vec<Value>>(&linea)
            .expect("datos de los focos")
            .into_iter()
            .map(|f| {
                (
                    f["port"].as_u64().expect("puerto") as u16,
                    f["key"].as_str().expect("clave").to_owned(),
                    serde_json::from_value(f["mapping"].clone()).expect("mapeo"),
                )
            })
            .collect();
        Self {
            proceso,
            entrada,
            salida,
            datos,
        }
    }

    /// Escrituras que recibió cada foco hasta ahora.
    fn escrituras(&mut self) -> Vec<u64> {
        writeln!(self.entrada).expect("el proceso de los focos se fue");
        let mut linea = String::new();
        self.salida.read_line(&mut linea).expect("sin respuesta");
        serde_json::from_str(&linea).expect("respuesta de los focos")
    }
}

impl Drop for Focos {
    fn drop(&mut self) {
        let _ = self.proceso.kill();
        let _ = self.proceso.wait();
    }
}

fn id_foco(n: usize) -> DeviceId {
    DeviceId(format!("openlightbancofoco{n:04}"))
}

// ---------- Captura sintética ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Escena {
    /// Cambia en cada cuadro (un video).
    Movil,
    /// Un solo cuadro y después nada, como ScreenCaptureKit con la pantalla quieta.
    Fija,
}

/// Bytes por fila del búfer de origen: con relleno, como los de ScreenCaptureKit.
fn paso(ancho: usize) -> usize {
    (ancho * 4).next_multiple_of(64) + 64
}

/// Búferes de origen precalculados (no cuentan en la medición): degradé que se desplaza, un
/// bloque de color y bandas negras.
fn escenas(ancho: usize, alto: usize) -> Vec<Vec<u8>> {
    (0..120)
        .map(|t| {
            let mut bgra = vec![0_u8; paso(ancho) * alto];
            for y in 0..alto {
                for x in 0..ancho {
                    let px = if !(alto / 8..alto - alto / 8).contains(&y) {
                        [0, 0, 0, 255]
                    } else if (x + t * 2) % ancho < ancho / 3 {
                        [30, (120 + t) as u8, 220, 255]
                    } else {
                        [
                            (x * 2 + t * 4) as u8,
                            (y * 4) as u8,
                            (255 - x * 2) as u8,
                            255,
                        ]
                    };
                    let i = y * paso(ancho) + x * 4;
                    bgra[i..i + 4].copy_from_slice(&px);
                }
            }
            bgra
        })
        .collect()
}

/// Lo mismo que hace `leer_cuadro` en la captura de macOS con el búfer del sistema.
fn leer_cuadro(origen: &[u8], ancho: usize, alto: usize) -> Frame {
    let compacto = capture::common::pack_rows(origen, ancho, alto, paso(ancho)).expect("medidas");
    Frame::new(ancho as u32, alto as u32, compacto).expect("cuadro")
}

struct Sintetica {
    escena: Escena,
    tarea: Option<JoinHandle<()>>,
}

impl ScreenSource for Sintetica {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(vec![DisplayInfo {
            id: DisplayId(MONITOR.into()),
            name: "Monitor del banco".into(),
            width: 1920,
            height: 1080,
            builtin: false,
            primary: true,
        }])
    }

    fn start(
        &mut self,
        _display: DisplayId,
        (ancho, alto): (u32, u32),
        fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
        let (ancho, alto) = (ancho as usize, alto as usize);
        let origenes = escenas(ancho, alto);
        let escena = self.escena;
        let (tx, rx) = watch::channel(None);
        let periodo = Duration::from_secs_f32(1.0 / fps.max(1.0));
        self.tarea = Some(tokio::spawn(async move {
            let mut reloj = tokio::time::interval(periodo);
            for origen in origenes.iter().cycle() {
                reloj.tick().await;
                if tx.send(Some(leer_cuadro(origen, ancho, alto))).is_err() {
                    return;
                }
                if escena == Escena::Fija {
                    tx.closed().await;
                    return;
                }
            }
        }));
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some(tarea) = self.tarea.take() {
            tarea.abort();
        }
    }

    fn permission(&self) -> PermissionState {
        PermissionState::Granted
    }

    fn request_permission(&self) -> PermissionState {
        PermissionState::Granted
    }
}

// ---------- Nube falsa ----------

struct Nube {
    focos: Vec<CloudDevice>,
    mapeos: HashMap<DeviceId, DpMapping>,
}

#[async_trait]
impl TuyaCloud for Nube {
    async fn validate(&self) -> Result<CloudAccount, CloudError> {
        Ok(CloudAccount {
            uid: "banco".into(),
            region: Region::Us,
        })
    }

    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError> {
        Ok(self.focos.clone())
    }

    async fn mapping(&self, device: &DeviceId) -> Result<DpMapping, CloudError> {
        self.mapeos
            .get(device)
            .cloned()
            .ok_or(CloudError::InvalidSignature)
    }
}

// ---------- Eventos ----------

/// Cuenta los eventos que llegan al destino de la app. Sin destino de verdad, además los
/// serializa (es lo que haría Tauri antes de mandarlos a la ventana) y cuenta los bytes.
#[derive(Default)]
struct Cuenta {
    destino: Option<Arc<dyn EventSink>>,
    eventos: Mutex<BTreeMap<&'static str, (u64, u64)>>,
}

impl Cuenta {
    fn foto(&self) -> BTreeMap<&'static str, (u64, u64)> {
        self.eventos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Bytes del payload de un evento de los que salen seguido (el resto casi no pesa).
fn tamano(event: &AppEvent) -> usize {
    match event {
        AppEvent::DeviceHealth(p) => serde_json::to_string(p),
        AppEvent::ColorLive(p) => serde_json::to_string(p),
        AppEvent::PipelineStats(p) => serde_json::to_string(p),
        _ => return 0,
    }
    .map_or(0, |s| s.len())
}

impl EventSink for Cuenta {
    fn emit(&self, event: AppEvent) {
        let bytes = if self.destino.is_none() {
            tamano(&event)
        } else {
            0
        };
        {
            let mut eventos = self.eventos.lock().unwrap_or_else(PoisonError::into_inner);
            let (n, b) = eventos.entry(event.name()).or_default();
            *n += 1;
            *b += bytes as u64;
        }
        if let Some(destino) = &self.destino {
            destino.emit(event);
        }
    }
}

// ---------- Armado ----------

#[derive(Clone)]
struct Mundo {
    carpeta: std::path::PathBuf,
    llavero: Arc<dyn SecretStore>,
    escena: Escena,
    focos: Vec<(u16, String, DpMapping)>,
}

impl Mundo {
    fn backend(&self, eventos: Arc<dyn EventSink>) -> Arc<Backend> {
        let nube_focos: Vec<CloudDevice> = (1..=FOCOS)
            .map(|n| CloudDevice {
                id: id_foco(n),
                name: format!("Foco del banco {n}"),
                local_key: Secret::new(self.focos[n - 1].1.clone()),
                product_id: "banco".into(),
                category: "dj".into(),
                model: None,
                online: true,
                cloud_ip: None,
            })
            .collect();
        let mapeos: HashMap<DeviceId, DpMapping> = (1..=FOCOS)
            .map(|n| (id_foco(n), self.focos[n - 1].2.clone()))
            .collect();
        let red: Vec<DiscoveredDevice> = (1..=FOCOS)
            .map(|n| DiscoveredDevice {
                id: id_foco(n),
                ip: "127.0.0.1".into(),
                version: ProtocolVersion::V35,
                product_key: None,
            })
            .collect();
        let puertos: HashMap<DeviceId, u16> = (1..=FOCOS)
            .map(|n| (id_foco(n), self.focos[n - 1].0))
            .collect();
        let escena = self.escena;
        Backend::new(BackendParts {
            data_dir: self.carpeta.clone(),
            secrets: self.llavero.clone(),
            events: eventos,
            sources: Arc::new(move || {
                Box::new(Sintetica {
                    escena,
                    tarea: None,
                })
            }),
            cloud: Arc::new(move |_: CloudCredentials| {
                Ok(Arc::new(Nube {
                    focos: nube_focos.clone(),
                    mapeos: mapeos.clone(),
                }) as Arc<dyn TuyaCloud>)
            }),
            scanner: Arc::new(FixedScanner(red)),
            locator: Arc::new(NoLocator),
            options: BackendOptions {
                list_scan: Duration::from_millis(50),
                lookup_scan: Duration::from_millis(50),
                device_tweak: Some(Arc::new(move |id, config| match puertos.get(id) {
                    Some(puerto) => config.port(*puerto),
                    None => config,
                })),
                ..BackendOptions::default()
            },
        })
        .expect("backend")
    }

    /// Carga las credenciales falsas, agrega los dos focos (uno por mitad de la pantalla) y da
    /// por terminado el asistente, para que la UI muestre el panel.
    async fn configurar(&self) {
        let backend = self.backend(Arc::new(Cuenta::default()));
        backend
            .cloud_validate(CloudCredentialsInput {
                access_id: "banco".into(),
                access_secret: "banco".into(),
                region: Region::Us,
            })
            .await
            .expect("nube falsa");
        for (n, mitad) in [(1, ZonePreset::Left), (2, ZonePreset::Right)] {
            backend
                .device_add(DeviceAddRequest {
                    id: id_foco(n),
                    name: format!("Foco del banco {n}"),
                    ip: Some("127.0.0.1".into()),
                })
                .await
                .expect("agregar el foco");
            backend
                .zones_set(
                    id_foco(n),
                    vec![Zone::from_preset(DisplayId(MONITOR.into()), mitad)],
                )
                .expect("zona");
        }
        use OnboardingStep as P;
        backend
            .onboarding_save(
                P::Done,
                vec![
                    P::Welcome,
                    P::Permissions,
                    P::TuyaAccount,
                    P::CloudProject,
                    P::LinkApp,
                    P::Credentials,
                    P::ChooseDevices,
                    P::TestDevices,
                    P::Zones,
                    P::Calibrate,
                    P::Done,
                ],
            )
            .expect("asistente");
    }
}

// ---------- Medición ----------

/// Segundos de CPU y memoria (MB) de un proceso, según `top`.
fn top(pid: u32) -> Option<(f64, f64)> {
    let salida = Command::new("top")
        .args([
            "-l",
            "1",
            "-pid",
            &pid.to_string(),
            "-stats",
            "pid,time,mem",
        ])
        .output()
        .ok()?;
    let texto = String::from_utf8_lossy(&salida.stdout);
    let fila = texto
        .lines()
        .rev()
        .find(|l| l.split_whitespace().next() == Some(&pid.to_string()))?;
    let mut campos = fila.split_whitespace().skip(1);
    let cpu = campos
        .next()?
        .trim_end_matches(['+', '-'])
        .split(':')
        .try_fold(0.0, |acc, p| p.parse::<f64>().ok().map(|v| acc * 60.0 + v))?;
    let memoria = campos.next()?.trim_end_matches(['+', '-']);
    let (numero, unidad) = memoria.split_at(memoria.len().checked_sub(1)?);
    let factor = match unidad {
        "B" => 1.0 / 1_048_576.0,
        "K" => 1.0 / 1024.0,
        "M" => 1.0,
        "G" => 1024.0,
        _ => return None,
    };
    Some((cpu, numero.parse::<f64>().ok()? * factor))
}

struct Muestras {
    pid: u32,
    inicio: Option<(Instant, f64)>,
    fin: Option<(Instant, f64)>,
    memoria: Vec<f64>,
}

impl Muestras {
    fn new(pid: u32) -> Self {
        Self {
            pid,
            inicio: None,
            fin: None,
            memoria: Vec::new(),
        }
    }

    fn tomar(&mut self) {
        if let Some((cpu, memoria)) = top(self.pid) {
            let ahora = (Instant::now(), cpu);
            self.inicio.get_or_insert(ahora);
            self.fin = Some(ahora);
            self.memoria.push(memoria);
        }
    }

    fn resumen(&self) -> Value {
        let cpu = match (self.inicio, self.fin) {
            (Some((t0, c0)), Some((t1, c1))) if t1 > t0 => {
                Some(100.0 * (c1 - c0) / (t1 - t0).as_secs_f64())
            }
            _ => None,
        };
        let n = self.memoria.len().max(1) as f64;
        let redondear = |v: f64| (v * 100.0).round() / 100.0;
        json!({
            "cpu_pct": cpu.map(redondear),
            "mem_mb_min": self.memoria.iter().copied().reduce(f64::min),
            "mem_mb_prom": redondear(self.memoria.iter().sum::<f64>() / n),
            "mem_mb_max": self.memoria.iter().copied().reduce(f64::max),
        })
    }
}

fn eventos_por_segundo(
    antes: &BTreeMap<&'static str, (u64, u64)>,
    despues: &BTreeMap<&'static str, (u64, u64)>,
    segundos: f64,
) -> Value {
    let mut salida = serde_json::Map::new();
    for (nombre, (n, bytes)) in despues {
        let (n0, b0) = antes.get(nombre).copied().unwrap_or_default();
        salida.insert(
            (*nombre).to_owned(),
            json!({
                "por_s": ((n - n0) as f64 / segundos * 10.0).round() / 10.0,
                "bytes_por_s": ((bytes - b0) as f64 / segundos).round(),
            }),
        );
    }
    Value::Object(salida)
}

/// Cuántos eventos que solo sirven para dibujar llegaron al destino.
fn de_dibujo(cuenta: &Cuenta) -> u64 {
    use openlight_types::ipc::events as nombre;
    let foto = cuenta.foto();
    [
        nombre::COLOR_LIVE,
        nombre::DEVICE_HEALTH,
        nombre::PIPELINE_STATS,
    ]
    .iter()
    .filter_map(|n| foto.get(n))
    .map(|(n, _)| n)
    .sum()
}

/// Muestra la ventana y la cierra (lo que hace el usuario) y cuenta los eventos de dibujo en cada
/// tramo. Cerrar pasa por `on_window_event`, como en la app.
fn ciclo(window: &tauri::WebviewWindow, cuenta: &Cuenta) -> Value {
    use tauri::Manager;
    let esperar = |ms| std::thread::sleep(Duration::from_millis(ms));
    let tramo = || {
        let antes = de_dibujo(cuenta);
        esperar(2000);
        de_dibujo(cuenta) - antes
    };
    let oculta = tramo();
    window
        .set_position(tauri::LogicalPosition::new(-30_000.0, -30_000.0))
        .expect("mover la ventana");
    window.show().expect("mostrar");
    openlight_lib::windows::sync_visibility(window.app_handle(), None);
    let visible = tramo();
    window.close().expect("cerrar");
    esperar(1000);
    let cerrada = tramo();
    json!({
        "oculta_en_2s": oculta,
        "visible_en_2s": visible,
        "cerrada_en_2s": cerrada,
    })
}

/// Mide durante `segundos` los procesos `pids`, el asignador, los eventos y los envíos.
fn medir(
    segundos: u64,
    pids: &[(&str, u32)],
    cuenta: &Cuenta,
    focos: &Mutex<Focos>,
) -> serde_json::Map<String, Value> {
    let mut muestras: Vec<(&str, Muestras)> = pids
        .iter()
        .map(|(nombre, pid)| (*nombre, Muestras::new(*pid)))
        .collect();
    let escrituras = || {
        focos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .escrituras()
    };
    muestras.iter_mut().for_each(|(_, m)| m.tomar());
    let eventos = cuenta.foto();
    let envios = escrituras();
    let (a0, b0) = (
        ASIGNACIONES.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    let inicio = Instant::now();
    while inicio.elapsed() < Duration::from_secs(segundos) {
        std::thread::sleep(Duration::from_secs(2));
        muestras.iter_mut().for_each(|(_, m)| m.tomar());
    }
    let dur = inicio.elapsed().as_secs_f64();
    let (a1, b1) = (
        ASIGNACIONES.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    let despues = cuenta.foto();
    let envios_despues = escrituras();

    let mut salida = serde_json::Map::new();
    salida.insert("segundos".into(), json!(dur.round()));
    for (nombre, m) in &muestras {
        salida.insert((*nombre).to_owned(), m.resumen());
    }
    // Las muestras de `top` también reservan memoria en este proceso, pero son una cada 2 s
    salida.insert(
        "asignaciones_por_s".into(),
        json!(((a1 - a0) as f64 / dur).round()),
    );
    salida.insert(
        "kb_asignados_por_s".into(),
        json!(((b1 - b0) as f64 / dur / 1024.0).round()),
    );
    salida.insert(
        "escrituras_por_s_y_foco".into(),
        json!(
            envios
                .iter()
                .zip(&envios_despues)
                .map(|(a, b)| ((b - a) as f64 / dur * 10.0).round() / 10.0)
                .collect::<Vec<_>>()
        ),
    );
    salida.insert(
        "eventos".into(),
        eventos_por_segundo(&eventos, &despues, dur),
    );
    salida
}

// ---------- Modos ----------

struct Opciones {
    segundos: u64,
    escena: Escena,
    ciclo: bool,
}

fn preparar(escena: Escena) -> (tempfile::TempDir, Mundo, Focos) {
    let focos = Focos::lanzar();
    let dir = tempfile::tempdir().expect("carpeta temporal");
    let mundo = Mundo {
        carpeta: dir.path().join("datos"),
        llavero: Arc::new(MemoryStore::new()),
        escena,
        focos: focos.datos.clone(),
    };
    (dir, mundo, focos)
}

fn pipeline(opciones: Opciones) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let (_dir, mundo, focos) = preparar(opciones.escena);
    let focos = Mutex::new(focos);
    runtime.block_on(mundo.configurar());
    let cuenta = Arc::new(Cuenta::default());
    let backend = mundo.backend(cuenta.clone());
    runtime
        .block_on(backend.session_start())
        .expect("arrancar la sesión");
    std::thread::sleep(CALENTAMIENTO);
    let resultado = medir(
        opciones.segundos,
        &[("proceso", std::process::id())],
        &cuenta,
        &focos,
    );
    println!("{}", Value::Object(resultado));
    runtime.block_on(backend.shutdown());
}

/// El PID del proceso WebContent de una ventana (`-[WKWebView _webProcessIdentifier]`).
#[cfg(target_os = "macos")]
fn pid_web(window: &tauri::WebviewWindow) -> Option<u32> {
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .with_webview(move |webview| {
            // SAFETY: `inner()` es el WKWebView de la ventana, vivo durante esta función, que
            // corre en el hilo principal; el selector devuelve un `pid_t`.
            let pid: i32 = unsafe {
                let view = &*webview.inner().cast::<objc2::runtime::AnyObject>();
                objc2::msg_send![view, _webProcessIdentifier]
            };
            let _ = tx.send(pid);
        })
        .ok()?;
    let pid = rx.recv_timeout(Duration::from_secs(10)).ok()?;
    u32::try_from(pid).ok().filter(|p| *p > 0)
}

#[cfg(not(target_os = "macos"))]
fn pid_web(_window: &tauri::WebviewWindow) -> Option<u32> {
    None
}

fn ventana(opciones: Opciones) {
    use tauri::Manager;

    let (_dir, mundo, focos) = preparar(opciones.escena);
    let focos = Arc::new(Mutex::new(focos));
    // Los focos y el asistente quedan listos antes de que la UI pregunte por ellos
    tauri::async_runtime::block_on(mundo.configurar());

    let mut context = openlight_lib::context();
    // Otro identificador: nada de lo que guarde la ventana se mezcla con la app instalada
    context.config_mut().identifier = "io.github.matiasm0rales.openlight.banco".into();
    for config in &mut context.config_mut().app.windows {
        config.visible = false;
    }

    let specta = openlight_lib::ipc::specta_builder();
    let (segundos, con_ciclo) = (opciones.segundos, opciones.ciclo);
    tauri::Builder::default()
        .on_window_event(openlight_lib::windows::on_window_event)
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            specta.mount_events(app);
            let cuenta = Arc::new(Cuenta {
                destino: Some(Arc::new(openlight_lib::events::TauriSink(
                    app.handle().clone(),
                ))),
                ..Cuenta::default()
            });
            let backend = mundo.backend(cuenta.clone());
            app.manage(backend.clone());
            // Igual que cuando la app arranca sola: la ventana queda escondida en el tray
            openlight_lib::windows::hide_main(app.handle())?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // Que la UI cargue antes de arrancar la sesión
                std::thread::sleep(Duration::from_secs(3));
                tauri::async_runtime::block_on(backend.session_start()).expect("sesión");
                std::thread::sleep(CALENTAMIENTO);
                let window = handle
                    .get_webview_window(openlight_lib::windows::MAIN)
                    .expect("ventana principal");
                let mut pids = vec![("proceso", std::process::id())];
                match pid_web(&window) {
                    Some(pid) => pids.push(("webcontent", pid)),
                    None => eprintln!("no se pudo saber el PID del WebContent"),
                }
                let mut resultado = medir(segundos, &pids, &cuenta, &focos);
                resultado.insert(
                    "ventana_visible".into(),
                    json!(window.is_visible().unwrap_or_default()),
                );
                if con_ciclo {
                    resultado.insert("ciclo".into(), ciclo(&window, &cuenta));
                }
                println!("{}", Value::Object(resultado));
                tauri::async_runtime::block_on(backend.shutdown());
                focos
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .proceso
                    .kill()
                    .ok();
                handle.exit(0);
            });
            Ok(())
        })
        .run(context)
        .expect("no arrancó Tauri");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let modo = args.next().unwrap_or_default();
    let mut opciones = Opciones {
        segundos: 30,
        escena: Escena::Movil,
        ciclo: false,
    };
    while let Some(arg) = args.next() {
        let valor = args.next().unwrap_or_default();
        match arg.as_str() {
            "--segundos" => opciones.segundos = valor.parse().expect("--segundos"),
            "--ciclo" => opciones.ciclo = valor == "si",
            "--escena" => {
                opciones.escena = match valor.as_str() {
                    "movil" => Escena::Movil,
                    "fija" => Escena::Fija,
                    _ => panic!("--escena movil|fija"),
                }
            }
            _ => panic!("opción desconocida: {arg}"),
        }
    }
    match modo.as_str() {
        "sim" => proceso_sim(),
        "pipeline" => pipeline(opciones),
        "ventana" => ventana(opciones),
        _ => eprintln!(
            "uso: banco pipeline|ventana [--segundos N] [--escena movil|fija] [--ciclo si]"
        ),
    }
}
