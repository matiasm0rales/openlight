//! El stream de ScreenCaptureKit y el hilo que lo supervisa.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};

use openlight_types::Frame;
use screencapturekit::cm::SCFrameStatus;
use screencapturekit::prelude::*;
use screencapturekit::stream::configuration::SCCaptureResolutionType;
use tokio::sync::watch;

use super::{REVISAR_CADA, displays};
use crate::common::{is_empty_frame, pack_rows};

type Salida = Arc<watch::Sender<Option<Frame>>>;

/// Qué capturar.
pub(crate) struct Params {
    pub display: u32,
    pub target: (u32, u32),
    pub fps: f32,
    pub exclude_own_app: bool,
}

/// Bucle del hilo de captura: mantiene un stream abierto mientras el monitor esté disponible y
/// publica `None` mientras no lo esté. Termina al recibir algo por `stop` (o si se cierra el
/// canal) o cuando ya nadie escucha el `watch`.
pub(crate) fn supervise(
    params: Params,
    tx: watch::Sender<Option<Frame>>,
    stop: mpsc::Receiver<()>,
) {
    let tx: Salida = Arc::new(tx);
    let mut activo: Option<Activo> = None;
    loop {
        let disponible = displays::is_available(params.display);
        let fallo = activo
            .as_ref()
            .is_some_and(|a| a.fallo.load(Ordering::Relaxed));
        if !disponible || fallo {
            if let Some(a) = activo.take() {
                a.cerrar();
            }
            sin_pantalla(&tx);
        }
        if disponible && activo.is_none() {
            // Si falla (permiso revocado, monitor que se fue justo ahora), se reintenta en la
            // próxima revisión.
            activo = abrir(&params, &tx).ok();
        }
        if tx.is_closed() {
            break;
        }
        match stop.recv_timeout(REVISAR_CADA) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    if let Some(a) = activo.take() {
        a.cerrar();
    }
    sin_pantalla(&tx);
}

/// Publica `None` solo si había un cuadro, para no despertar de más a quien escucha.
fn sin_pantalla(tx: &Salida) {
    tx.send_if_modified(|v| v.take().is_some());
}

struct Activo {
    stream: SCStream,
    fallo: Arc<AtomicBool>,
}

impl Activo {
    fn cerrar(self) {
        // Si el stream ya se cortó solo, `stop_capture` falla y no importa.
        let _ = self.stream.stop_capture();
    }
}

fn abrir(params: &Params, tx: &Salida) -> Result<Activo, String> {
    let contenido = SCShareableContent::get().map_err(|e| e.to_string())?;
    let display = contenido
        .displays()
        .into_iter()
        .find(|d| d.display_id() == params.display)
        .ok_or_else(|| format!("ScreenCaptureKit no ve el monitor {}", params.display))?;

    let apps = contenido.applications();
    let pid = std::process::id();
    let propias: Vec<&SCRunningApplication> = if params.exclude_own_app {
        apps.iter()
            .filter(|a| u32::try_from(a.process_id()) == Ok(pid))
            .collect()
    } else {
        Vec::new()
    };
    let filtro = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_applications(&propias, &[])
        .build()
        .map_err(|e| e.to_string())?;

    let (ancho, alto) = params.target;
    // 1/fps como fracción: 1000 / (fps * 1000) s.
    let escala = (f64::from(params.fps) * 1000.0)
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as i32;
    let intervalo = CMTime::new(1000, escala);
    let mut config = SCStreamConfiguration::new()
        .with_width(ancho)
        .with_height(alto)
        .with_pixel_format(PixelFormat::BGRA)
        .with_scales_to_fit(true)
        .with_minimum_frame_interval(&intervalo)
        // Lo mínimo que acepta: guardar cuadros viejos no sirve, solo importa el último.
        .with_queue_depth(3)
        .with_shows_cursor(false);
    // Disponibles desde macOS 14; en versiones anteriores fallan sin cambiar nada.
    // Sin esto, un monitor 16:10 reducido a 16:9 sale con franjas negras a los costados.
    let _ = config.set_preserves_aspect_ratio(false);
    // La reducción parte de la resolución en puntos y no de la Retina: menos trabajo de GPU
    // para el mismo resultado a 96×54.
    let _ = config.set_capture_resolution_type(SCCaptureResolutionType::Nominal);

    let fallo = Arc::new(AtomicBool::new(false));
    let mut stream = SCStream::new_with_delegate(
        &filtro,
        &config,
        Delegado {
            fallo: Arc::clone(&fallo),
        },
    )
    .map_err(|e| e.to_string())?;
    stream
        .add_output_handler(Receptor { tx: Arc::clone(tx) }, SCStreamOutputType::Screen)
        .map_err(|e| e.to_string())?;
    stream.start_capture().map_err(|e| e.to_string())?;
    Ok(Activo { stream, fallo })
}

/// Avisa al supervisor si el sistema corta el stream (monitor desconectado, permiso revocado).
struct Delegado {
    fallo: Arc<AtomicBool>,
}

impl SCStreamDelegateTrait for Delegado {
    fn did_stop_with_error(&self, _error: SCError) {
        self.fallo.store(true, Ordering::Relaxed);
    }
}

/// Recibe los cuadros (en una cola de ScreenCaptureKit) y publica el último.
struct Receptor {
    tx: Salida,
}

impl SCStreamOutputTrait for Receptor {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if !matches!(of_type, SCStreamOutputType::Screen) {
            return;
        }
        match sample.frame_status() {
            Some(SCFrameStatus::Complete) => {}
            // La pantalla dejó de entregar cuadros: no hay pantalla.
            Some(SCFrameStatus::Suspended | SCFrameStatus::Stopped) => {
                sin_pantalla(&self.tx);
                return;
            }
            // `Idle` = nada cambió: vale el último cuadro publicado.
            _ => return,
        }
        let Some(cuadro) = leer_cuadro(&sample) else {
            return;
        };
        if is_empty_frame(&cuadro.bgra) {
            sin_pantalla(&self.tx);
        } else {
            self.tx.send_replace(Some(cuadro));
        }
    }
}

fn leer_cuadro(sample: &CMSampleBuffer) -> Option<Frame> {
    let pixeles = sample.pixel_buffer()?;
    let guard = pixeles.lock_read_only().ok()?;
    let (ancho, alto, fila) = (guard.width(), guard.height(), guard.bytes_per_row());
    // SAFETY: el búfer queda bloqueado para lectura mientras viva `guard`, que dura más que
    // `datos`; ScreenCaptureKit no lo reutiliza hasta que soltamos el `CMSampleBuffer`, y no
    // lo desbloqueamos a mano.
    let datos = unsafe { guard.as_slice() }?;
    let compacto = pack_rows(datos, ancho, alto, fila)?;
    Frame::new(
        u32::try_from(ancho).ok()?,
        u32::try_from(alto).ok()?,
        compacto,
    )
    .ok()
}
