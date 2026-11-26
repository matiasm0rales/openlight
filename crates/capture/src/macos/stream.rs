//! El stream de ScreenCaptureKit y el hilo que lo supervisa.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};

use openlight_types::Frame;
use screencapturekit::cm::SCFrameStatus;
use screencapturekit::error::{SCError, SCStreamErrorCode};
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
///
/// Por `primer_intento` avisa una sola vez, tras la primera vuelta, si el sistema negó el permiso.
pub(crate) fn supervise(
    params: Params,
    tx: watch::Sender<Option<Frame>>,
    stop: mpsc::Receiver<()>,
    primer_intento: mpsc::SyncSender<bool>,
) {
    let mut primer_intento = Some(primer_intento);
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
        let mut sin_permiso = false;
        if disponible && activo.is_none() {
            // Si falla (permiso revocado, monitor que se fue justo ahora), se reintenta en la
            // próxima revisión.
            match abrir(&params, &tx) {
                Ok(a) => activo = Some(a),
                Err(error) => sin_permiso = es_negativa_de_permiso(&error),
            }
        }
        if let Some(aviso) = primer_intento.take() {
            // Si quien arrancó la captura ya no espera, no importa.
            let _ = aviso.send(sin_permiso);
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

/// Si ScreenCaptureKit negó la captura por falta de permiso.
///
/// El código `UserDeclined` solo llega por el delegado: al pedir el contenido o arrancar el stream
/// el puente entrega nada más que el texto, en el idioma del sistema. Todas sus traducciones
/// nombran «TCC» (el subsistema de permisos de macOS).
fn es_negativa_de_permiso(error: &SCError) -> bool {
    error.stream_error_code() == Some(SCStreamErrorCode::UserDeclined)
        || error.to_string().contains("TCC")
}

fn abrir(params: &Params, tx: &Salida) -> Result<Activo, SCError> {
    let contenido = SCShareableContent::get()?;
    let display = contenido
        .displays()
        .into_iter()
        .find(|d| d.display_id() == params.display)
        .ok_or_else(|| {
            SCError::DisplayNotFound(format!(
                "ScreenCaptureKit no ve el monitor {}",
                params.display
            ))
        })?;

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
        .build()?;

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
    )?;
    stream.add_output_handler(Receptor { tx: Arc::clone(tx) }, SCStreamOutputType::Screen)?;
    stream.start_capture()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // Los textos con los que macOS niega la captura (`SCSTREAM_ERROR_USER_DECLINED` de ReplayKit)
    // están traducidos al idioma del sistema; todos nombran «TCC».
    #[test]
    fn la_negativa_se_reconoce_en_cualquier_idioma() {
        for texto in [
            "The user declined TCCs for application, window, display capture",
            "El usuario rechazó la configuración de TCC para la captura de pantallas, ventanas y aplicaciones",
            "Benutzer:in hat TCCs für die Aufnahme durch Apps, Fenster, Displays abgelehnt",
            "ユーザがアプリケーション、ウインドウ、ディスプレイ取り込みのTCCを拒否しました",
        ] {
            // Así llegan del puente en Swift: sin código, solo con el texto
            for error in [
                SCError::NoShareableContent(format!("Content unavailable: {texto}")),
                SCError::CaptureStartFailed(format!("Stream error: {texto}")),
            ] {
                assert!(es_negativa_de_permiso(&error), "{error}");
            }
        }
    }

    #[test]
    fn la_negativa_tambien_se_reconoce_por_su_codigo() {
        assert!(es_negativa_de_permiso(&SCError::from_error_code(-3801)));
    }

    #[test]
    fn otras_fallas_no_son_falta_de_permiso() {
        for error in [
            SCError::NoShareableContent(
                "Content unavailable: The operation couldn’t be completed".into(),
            ),
            SCError::CaptureStartFailed("Stream error: Failed to start the stream".into()),
            SCError::DisplayNotFound("ScreenCaptureKit no ve el monitor 1".into()),
            // -3802: no se pudo arrancar
            SCError::from_error_code(-3802),
        ] {
            assert!(!es_negativa_de_permiso(&error), "{error}");
        }
    }
}
