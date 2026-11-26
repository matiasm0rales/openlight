//! Banco de rendimiento (criterio de `docs/PLAN.md` 4.3): la app de verdad con todo lo de afuera
//! falso. Dos focos `tuya-sim` en un proceso aparte (así su CPU no cuenta), una captura
//! sintética de 96×54 que recorre el mismo camino que la de macOS, llavero en memoria y carpeta
//! de datos temporal. **No toca focos, red, pantalla ni llavero reales.**
//!
//! ```text
//! # Solo Rust: CPU, asignaciones y eventos del pipeline por segundo
//! cargo run --release -p openlight --example banco -- pipeline [--segundos 30] [--escena movil|fija]
//!
//! # Con Tauri y la UI de verdad, sin ventana (como cuando la app arranca sola): CPU y memoria
//! pnpm --filter openlight-ui build
//! cargo run --release -p openlight --features tauri/custom-protocol --example banco -- ventana [--segundos 60]
//!
//! # Además, abre la ventana como desde el tray (aparece un momento y saca el foco) y la cierra:
//! # que los eventos de dibujo vuelven y se cortan, y que la ventana nueva muestra todo
//! ... banco ventana --segundos 10 --ciclo si
//!
//! # Con el Panel a la vista, o después de cerrar la ventana
//! ... banco ventana --estado visible [--posicion borde|entera]
//! ... banco ventana --estado cerrada
//! ```
//!
//! La CPU está en % de un núcleo (como `top`) y la memoria es la columna `MEM` de `top`. Las
//! mediciones usan `top` y por eso solo salen en macOS.
//!
//! Se miden el proceso de la app, el WebContent y los demás procesos de WebKit que aparecen al
//! arrancar (GPU y red; si otra app abre uno mientras tanto, se cuela). `total` suma la app y el
//! WebContent; `total_webkit`, todo.
//!
//! `--estado visible` **muestra una ventana** encima de las demás, sin sacar el foco: con
//! `--posicion borde` (la de siempre) asoma solo una franja de 40 px abajo a la derecha, que
//! alcanza para que WebKit la dibuje entera; `entera` la centra. Para buscar qué consume están
//! `--js archivo` (se evalúa en la ventana antes de medir) y `--sin color,salud,estadisticas`
//! (esos eventos no se mandan).

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
    /// Eventos que no se le mandan a la ventana (para ver cuánto cuesta cada uno).
    descartados: Vec<&'static str>,
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
        if let Some(destino) = &self.destino
            && !self.descartados.contains(&event.name())
        {
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

/// Lo que muestra la página: para ver que se dibuja y que una ventana nueva recibe todo.
const PAGINA: &str = "({ruta: location.hash, oculta: document.hidden, \
    sesion: document.querySelector('.session .state')?.textContent, \
    focos: [...document.querySelectorAll('.card.device .chip')].map(c => c.textContent), \
    colores: [...document.querySelectorAll('.eclipse')].map(e => e.style.getPropertyValue('--core')), \
    ancho: innerWidth, alto: innerHeight, escala: devicePixelRatio})";

/// Cierra la ventana (si la hay), la abre como desde el tray y la vuelve a cerrar, y cuenta los
/// eventos de dibujo en cada tramo. Cerrar pasa por `on_window_event`, como en la app.
fn ciclo(app: &tauri::AppHandle, cuenta: &Cuenta) -> Value {
    use tauri::Manager;
    let esperar = |ms| std::thread::sleep(Duration::from_millis(ms));
    let tramo = || {
        let antes = de_dibujo(cuenta);
        esperar(2000);
        de_dibujo(cuenta) - antes
    };
    let ventana = || app.get_webview_window(openlight_lib::windows::MAIN);
    let cerrar = || {
        if let Some(window) = ventana() {
            window.close().expect("cerrar");
        }
        esperar(1000);
    };
    cerrar();
    let sin_ventana = tramo();
    openlight_lib::windows::show_main(app);
    esperar(3000);
    let abierta = tramo();
    let pagina = ventana().map(|w| evaluar(&w, PAGINA));
    cerrar();
    let cerrada = tramo();
    json!({
        "sin_ventana_en_2s": sin_ventana,
        "abierta_en_2s": abierta,
        "pagina": pagina,
        "cerrada_en_2s": cerrada,
        "ventana_al_final": ventana().is_some(),
    })
}

/// Los procesos de WebKit que hay ahora: PID y qué es.
fn procesos_webkit() -> BTreeMap<u32, &'static str> {
    let Ok(salida) = Command::new("ps").args(["-axo", "pid=,comm="]).output() else {
        return BTreeMap::new();
    };
    String::from_utf8_lossy(&salida.stdout)
        .lines()
        .filter_map(|linea| {
            let (pid, ruta) = linea.trim().split_once(' ')?;
            let tipo = match ruta.rsplit('/').next()? {
                "com.apple.WebKit.WebContent" => "webcontent",
                "com.apple.WebKit.GPU" => "gpu",
                "com.apple.WebKit.Networking" => "red",
                _ => return None,
            };
            Some((pid.parse().ok()?, tipo))
        })
        .collect()
}

/// Suma la CPU y la memoria media de los procesos `nombres` (uno que ya no está suma cero).
fn sumar(resultado: &serde_json::Map<String, Value>, nombres: &[String]) -> Value {
    let suma = |campo: &str| {
        let total: f64 = nombres
            .iter()
            .filter_map(|n| resultado.get(n)?.get(campo)?.as_f64())
            .sum();
        (total * 100.0).round() / 100.0
    };
    json!({ "cpu_pct": suma("cpu_pct"), "mem_mb_prom": suma("mem_mb_prom") })
}

/// Mide durante `segundos` los procesos `pids`, el asignador, los eventos y los envíos.
fn medir(
    segundos: u64,
    pids: &[(String, u32)],
    cuenta: &Cuenta,
    focos: &Mutex<Focos>,
) -> serde_json::Map<String, Value> {
    let mut muestras: Vec<(&str, Muestras)> = pids
        .iter()
        .map(|(nombre, pid)| (nombre.as_str(), Muestras::new(*pid)))
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

/// Cómo está la ventana mientras se mide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Estado {
    /// Como cuando la app arranca sola al iniciar sesión.
    SinAbrir,
    /// Con el Panel a la vista.
    Visible,
    /// Se mostró y se cerró, como hace el usuario.
    Cerrada,
}

struct Opciones {
    segundos: u64,
    escena: Escena,
    ciclo: bool,
    estado: Estado,
    entera: bool,
    js: Option<String>,
    sin: Vec<&'static str>,
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
        &[("proceso".to_owned(), std::process::id())],
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

/// Muestra la ventana sin sacar el foco, encima de las demás: asomando una franja abajo a la
/// derecha, o entera.
fn mostrar(window: &tauri::WebviewWindow, entera: bool) {
    use tauri::Manager;

    let monitor = window
        .primary_monitor()
        .expect("monitores")
        .expect("monitor principal");
    let pantalla = monitor.size().to_logical::<f64>(monitor.scale_factor());
    let ventana = window
        .outer_size()
        .expect("tamaño")
        .to_logical::<f64>(monitor.scale_factor());
    let posicion = if entera {
        tauri::LogicalPosition::new(
            (pantalla.width - ventana.width) / 2.0,
            (pantalla.height - ventana.height) / 2.0,
        )
    } else {
        tauri::LogicalPosition::new(pantalla.width - ventana.width, pantalla.height - 40.0)
    };
    window.set_always_on_top(true).expect("encima");
    window
        .set_visible_on_all_workspaces(true)
        .expect("en todos los escritorios");
    window.set_position(posicion).expect("mover la ventana");
    window.show().expect("mostrar");
    // En el hilo principal, como cuando se muestra desde el menú del tray
    let app = window.app_handle().clone();
    window
        .app_handle()
        .run_on_main_thread(move || openlight_lib::windows::sync_visibility(&app, None))
        .expect("hilo principal");
}

/// Evalúa `js` en la ventana y devuelve el resultado como JSON.
fn evaluar(window: &tauri::WebviewWindow, js: &str) -> Value {
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .eval_with_callback(js, move |respuesta| {
            let _ = tx.send(respuesta);
        })
        .expect("evaluar");
    rx.recv_timeout(Duration::from_secs(5))
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or(Value::Null)
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
    // Que la ventana nazca escondida y no saque el foco
    for config in &mut context.config_mut().app.windows {
        config.visible = false;
        config.focus = false;
    }

    let previos = procesos_webkit();
    let specta = openlight_lib::ipc::specta_builder();
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
                descartados: opciones.sin.clone(),
                ..Cuenta::default()
            });
            let backend = mundo.backend(cuenta.clone());
            app.manage(backend.clone());
            // Como cuando la app arranca sola: sin ventana, no hay a quién dibujarle
            openlight_lib::windows::sync_visibility(app.handle(), None);
            let window = match opciones.estado {
                Estado::SinAbrir => None,
                Estado::Visible | Estado::Cerrada => {
                    let config = &app.config().app.windows[0];
                    Some(tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?)
                }
            };
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let esperar = |s| std::thread::sleep(Duration::from_secs(s));
                // Que la UI cargue antes de arrancar la sesión
                esperar(3);
                tauri::async_runtime::block_on(backend.session_start()).expect("sesión");
                let web = window.as_ref().and_then(pid_web);
                let mut pagina = None;
                if let Some(window) = window {
                    mostrar(&window, opciones.entera);
                    if let Some(js) = &opciones.js {
                        esperar(2);
                        window.eval(js.as_str()).expect("evaluar --js");
                    }
                    esperar(3);
                    pagina = Some(evaluar(&window, PAGINA));
                    if opciones.estado == Estado::Cerrada {
                        window.close().expect("cerrar");
                        // Lo que tarda WebKit en soltar el proceso de una ventana destruida
                        esperar(10);
                    }
                }
                esperar(CALENTAMIENTO.as_secs());

                let nuevos: BTreeMap<u32, &str> = procesos_webkit()
                    .into_iter()
                    .filter(|(pid, _)| !previos.contains_key(pid))
                    .collect();
                let mut pids = vec![("proceso".to_owned(), std::process::id())];
                match web {
                    Some(pid) => pids.push(("webcontent".to_owned(), pid)),
                    None if opciones.estado != Estado::SinAbrir => {
                        eprintln!("no se pudo saber el PID del WebContent");
                    }
                    None => {}
                }
                for (pid, tipo) in &nuevos {
                    if Some(*pid) != web {
                        pids.push((format!("{tipo}_{pid}"), *pid));
                    }
                }
                let mut resultado = medir(opciones.segundos, &pids, &cuenta, &focos);
                let nombres: Vec<String> = pids.iter().map(|(n, _)| n.clone()).collect();
                let app_y_web: Vec<String> = nombres
                    .iter()
                    .filter(|n| *n == "proceso" || *n == "webcontent")
                    .cloned()
                    .collect();
                let total = sumar(&resultado, &app_y_web);
                let total_webkit = sumar(&resultado, &nombres);
                resultado.insert("total".into(), total);
                resultado.insert("total_webkit".into(), total_webkit);
                if let Some(pid) = web {
                    resultado.insert(
                        "webcontent_vivo".into(),
                        json!(procesos_webkit().contains_key(&pid)),
                    );
                }
                if let Some(pagina) = pagina {
                    resultado.insert("pagina".into(), pagina);
                }
                let window = handle.get_webview_window(openlight_lib::windows::MAIN);
                resultado.insert(
                    "ventana".into(),
                    json!(match &window {
                        None => "destruida",
                        Some(w) if w.is_visible().unwrap_or_default() => "visible",
                        Some(_) => "oculta",
                    }),
                );
                if opciones.ciclo {
                    resultado.insert("ciclo".into(), ciclo(&handle, &cuenta));
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
        .build(context)
        .expect("no arrancó Tauri")
        .run(|_, event| openlight_lib::windows::keep_running(&event));
}

fn main() {
    let mut args = std::env::args().skip(1);
    let modo = args.next().unwrap_or_default();
    let mut opciones = Opciones {
        segundos: 30,
        escena: Escena::Movil,
        ciclo: false,
        estado: Estado::SinAbrir,
        entera: false,
        js: None,
        sin: Vec::new(),
    };
    while let Some(arg) = args.next() {
        let valor = args.next().unwrap_or_default();
        match arg.as_str() {
            "--segundos" => opciones.segundos = valor.parse().expect("--segundos"),
            "--ciclo" => opciones.ciclo = valor == "si",
            "--estado" => {
                opciones.estado = match valor.as_str() {
                    "sin-abrir" => Estado::SinAbrir,
                    "visible" => Estado::Visible,
                    "cerrada" => Estado::Cerrada,
                    _ => panic!("--estado sin-abrir|visible|cerrada"),
                }
            }
            "--posicion" => opciones.entera = valor == "entera",
            "--js" => opciones.js = Some(std::fs::read_to_string(&valor).expect("--js")),
            "--sin" => {
                use openlight_types::ipc::events as nombre;
                opciones.sin = valor
                    .split(',')
                    .map(|evento| match evento {
                        "color" => nombre::COLOR_LIVE,
                        "salud" => nombre::DEVICE_HEALTH,
                        "estadisticas" => nombre::PIPELINE_STATS,
                        _ => panic!("--sin color,salud,estadisticas"),
                    })
                    .collect();
            }
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
            "uso: banco pipeline|ventana [--segundos N] [--escena movil|fija] [--ciclo si] \
             [--estado sin-abrir|visible|cerrada] [--posicion borde|entera] [--js archivo] \
             [--sin color,salud,estadisticas]"
        ),
    }
}
