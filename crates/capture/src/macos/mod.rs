//! Backend de captura para macOS con ScreenCaptureKit (tarea E1, sección 3.6 de `docs/PLAN.md`).
//!
//! - La GPU reduce la pantalla a `target` (~96×54): `SCStreamConfiguration` con ese tamaño y
//!   `minimumFrameInterval = 1/fps`. A la CPU solo llegan ~20 KB por cuadro.
//! - Cada cuadro se publica en un `watch`, que guarda solo el último. `None` = no hay pantalla.
//! - Un hilo supervisor revisa cada [`REVISAR_CADA`] qué monitores hay (como `Captura._revisar`
//!   del script): si el monitor seguido se va o se duerme, corta el stream y publica `None`; si
//!   vuelve, lo arranca de nuevo.
//! - Si CoreGraphics dice que hay permiso pero ScreenCaptureKit lo niega (el permiso quedó atado
//!   a otra versión de la app), `start` falla con [`CaptureError::PermissionStale`].
//! - Las ventanas de la propia app se excluyen del filtro (se puede apagar con
//!   [`MacosCapture::set_exclude_own_app`]).

// FFI: CoreGraphics no tiene una versión segura de `CGGetActiveDisplayList` (escribe en un
// arreglo que le pasamos) y `screencapturekit` marca como `unsafe` la lectura de los píxeles
// del `CVPixelBuffer` bloqueado. Cada bloque `unsafe` tiene su `SAFETY:` al lado.
#![allow(unsafe_code)]

mod displays;
mod stream;

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};
use openlight_types::{CaptureError, DisplayId, DisplayInfo, Frame, PermissionState, ScreenSource};
use tokio::sync::watch;

pub use displays::active_displays;

/// Cada cuánto el supervisor revisa qué monitores hay. El script lo hacía cada 3 s; revisar
/// cuesta microsegundos, así que acá es más seguido para reaccionar rápido al enchufar.
pub const REVISAR_CADA: Duration = Duration::from_millis(1000);

/// Cuánto espera [`MacosCapture::start`] a saber si el sistema negó la captura. Suele tardar
/// milisegundos.
const ESPERA_PRIMER_INTENTO: Duration = Duration::from_secs(3);

/// Implementación de [`ScreenSource`] con ScreenCaptureKit.
pub struct MacosCapture {
    worker: Option<Worker>,
    exclude_own_app: bool,
    /// Si en este proceso ya se pidió el permiso (para distinguir "denegado" de "sin pedir").
    requested: std::sync::atomic::AtomicBool,
}

struct Worker {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

impl MacosCapture {
    /// Crea la fuente sin capturar nada todavía. En macOS no falla; devuelve `Result` por la
    /// convención común a los tres backends (`PlatformCapture::new()`).
    pub fn new() -> Result<Self, CaptureError> {
        Ok(Self {
            worker: None,
            exclude_own_app: true,
            requested: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Si se excluyen las ventanas de este proceso (por defecto, sí). Por ejemplo, la pantalla
    /// de calibración no tiene que teñir el color que se mide. Vale desde el próximo `start`.
    pub fn set_exclude_own_app(&mut self, exclude: bool) {
        self.exclude_own_app = exclude;
    }

    /// Si hay una captura en curso.
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }
}

/// Qué devuelve `start` según el primer intento de abrir el stream (`true`: ScreenCaptureKit negó
/// el permiso aunque CoreGraphics decía que estaba). Si no hay respuesta a tiempo se sigue: el
/// supervisor reintenta solo.
fn primer_intento(aviso: &mpsc::Receiver<bool>, espera: Duration) -> Result<(), CaptureError> {
    match aviso.recv_timeout(espera) {
        Ok(true) => Err(CaptureError::PermissionStale),
        _ => Ok(()),
    }
}

/// Convierte un [`DisplayId`] de macOS (el `CGDirectDisplayID` en decimal) a número.
pub fn parse_display_id(id: &DisplayId) -> Result<u32, CaptureError> {
    id.0.parse()
        .map_err(|_| CaptureError::DisplayNotFound(id.clone()))
}

impl ScreenSource for MacosCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(active_displays())
    }

    fn start(
        &mut self,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
        if !(fps.is_finite() && fps > 0.0) || target.0 == 0 || target.1 == 0 {
            return Err(CaptureError::Backend(format!(
                "parámetros inválidos: {}×{} a {fps} fps",
                target.0, target.1
            )));
        }
        let cg_id = parse_display_id(&display)?;
        if !displays::active_ids().contains(&cg_id) {
            return Err(CaptureError::DisplayNotFound(display));
        }
        if !CGPreflightScreenCaptureAccess() {
            return Err(CaptureError::PermissionDenied);
        }
        self.stop();

        let (tx, rx) = watch::channel(None);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (aviso_tx, aviso_rx) = mpsc::sync_channel(1);
        let params = stream::Params {
            display: cg_id,
            target,
            fps,
            exclude_own_app: self.exclude_own_app,
        };
        let handle = std::thread::Builder::new()
            .name(format!("captura-{cg_id}"))
            .spawn(move || stream::supervise(params, tx, stop_rx, aviso_tx))
            .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
        self.worker = Some(Worker {
            stop: stop_tx,
            handle,
        });
        if let Err(error) = primer_intento(&aviso_rx, ESPERA_PRIMER_INTENTO) {
            self.stop();
            return Err(error);
        }
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some(worker) = self.worker.take() {
            // Si el hilo ya terminó, el envío falla y no importa.
            let _ = worker.stop.send(());
            let _ = worker.handle.join();
        }
    }

    fn permission(&self) -> PermissionState {
        if CGPreflightScreenCaptureAccess() {
            PermissionState::Granted
        } else if self.requested.load(std::sync::atomic::Ordering::Relaxed) {
            PermissionState::Denied
        } else {
            // CoreGraphics no distingue "nunca se pidió" de "denegado": hasta pedirlo en este
            // proceso no se sabe. Pedirlo de nuevo no molesta (macOS muestra el diálogo una
            // sola vez y después solo contesta).
            PermissionState::NotDetermined
        }
    }

    fn request_permission(&self) -> PermissionState {
        self.requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if CGRequestScreenCaptureAccess() {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }
}

impl Drop for MacosCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_id_se_lee_en_decimal() {
        assert_eq!(
            parse_display_id(&DisplayId("69733382".into())),
            Ok(69_733_382)
        );
        assert_eq!(
            parse_display_id(&DisplayId("DISPLAY1".into())),
            Err(CaptureError::DisplayNotFound(DisplayId("DISPLAY1".into())))
        );
    }

    #[test]
    fn rechaza_parametros_invalidos_sin_tocar_la_pantalla() {
        let mut fuente = MacosCapture::new().unwrap();
        let id = DisplayId("1".into());
        assert!(matches!(
            fuente.start(id.clone(), (0, 54), 20.0),
            Err(CaptureError::Backend(_))
        ));
        assert!(matches!(
            fuente.start(id, (96, 54), 0.0),
            Err(CaptureError::Backend(_))
        ));
        assert!(!fuente.is_running());
    }

    #[test]
    fn un_monitor_que_no_existe_es_display_not_found() {
        let mut fuente = MacosCapture::new().unwrap();
        // 0 nunca es un CGDirectDisplayID válido.
        let id = DisplayId("0".into());
        assert_eq!(
            fuente.start(id.clone(), (96, 54), 20.0).err(),
            Some(CaptureError::DisplayNotFound(id))
        );
    }

    #[test]
    fn si_el_primer_intento_es_negado_el_permiso_es_fantasma() {
        let (aviso, espera) = mpsc::sync_channel(1);
        aviso.send(true).unwrap();
        assert_eq!(
            primer_intento(&espera, Duration::from_secs(1)),
            Err(CaptureError::PermissionStale)
        );
    }

    #[test]
    fn cualquier_otro_primer_intento_deja_seguir_al_supervisor() {
        let (aviso, espera) = mpsc::sync_channel(1);
        aviso.send(false).unwrap();
        assert_eq!(primer_intento(&espera, Duration::from_secs(1)), Ok(()));
        // ScreenCaptureKit tarda en contestar, o el hilo terminó sin avisar
        assert_eq!(primer_intento(&espera, Duration::from_millis(10)), Ok(()));
        drop(aviso);
        assert_eq!(primer_intento(&espera, Duration::from_secs(1)), Ok(()));
    }

    #[test]
    fn stop_sin_captura_no_hace_nada() {
        let mut fuente = MacosCapture::new().unwrap();
        fuente.stop();
        fuente.stop();
        assert!(!fuente.is_running());
    }

    #[test]
    fn listar_monitores_no_necesita_permiso() {
        // En el CI puede no haber ninguno (runner sin pantalla); solo tiene que no fallar y
        // tener a lo sumo un principal.
        let pantallas = MacosCapture::new().unwrap().displays().unwrap();
        assert!(pantallas.iter().filter(|d| d.primary).count() <= 1);
        for d in &pantallas {
            assert!(parse_display_id(&d.id).is_ok());
            assert!(!d.name.is_empty());
        }
    }

    #[test]
    fn el_permiso_se_consulta_sin_pedirlo() {
        let estado = MacosCapture::new().unwrap().permission();
        assert_ne!(estado, PermissionState::NotApplicable);
    }

    // Necesita el permiso de Grabación de pantalla (los runners de macOS del CI no lo tienen) y
    // un monitor despierto: macOS saca de la lista activa a los que están dormidos.
    // Correr a mano con `cargo test -p capture -- --ignored`.
    #[test]
    #[ignore = "necesita permiso de Grabación de pantalla y una pantalla despierta"]
    fn captura_cuadros_reales_del_monitor_elegido() {
        let mut fuente = MacosCapture::new().unwrap();
        let pantallas = fuente.displays().unwrap();
        let id = crate::common::pick_display(None, &pantallas).expect("sin monitores");
        let mut rx = fuente.start(id, (96, 54), 20.0).unwrap();
        let inicio = std::time::Instant::now();
        while rx.borrow_and_update().is_none() {
            assert!(
                inicio.elapsed() < Duration::from_secs(5),
                "no llegó ningún cuadro"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let cuadro = rx.borrow().clone().unwrap();
        assert_eq!((cuadro.width, cuadro.height), (96, 54));
        assert_eq!(cuadro.bgra.len(), 96 * 54 * 4);
        assert!(cuadro.bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
        fuente.stop();
        assert!(!fuente.is_running());
    }
}
