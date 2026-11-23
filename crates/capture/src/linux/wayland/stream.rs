//! Lectura de cuadros por PipeWire.
//!
//! Por qué FFI directo: los crates seguros `pipewire`/`libspa` no compilan a la vez contra
//! PipeWire 0.3.48 (Ubuntu 22.04, el runner del CI y la base habitual de los builds de Linux) y
//! contra PipeWire 1.x: desde la 0.8 usan campos que 0.3.48 no tiene, y la 0.7 no tiene campos
//! que 1.x exige. Los crates `-sys` en cambio se generan con bindgen contra los headers del
//! sistema y sirven para las dos. Acá se usan solo funciones estables de la API pública de
//! libpipewire; el POD de formato se arma y se lee en Rust seguro (`pod.rs`).
//!
//! Se pide video crudo de 32 bits en memoria compartida (sin modificadores DMA-BUF, así el
//! compositor entrega búferes mapeables por la CPU). Cada cuadro se reduce con una grilla rala
//! de puntos ([`downscale`]) y se publica a lo sumo a `fps`.

#![allow(unsafe_code)] // FFI con libpipewire; ver la documentación del módulo.

use std::ffi::{CStr, c_char, c_void};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use libspa_sys as spa;
use openlight_types::{CaptureError, Frame};
use pipewire_sys as pw;
use tokio::sync::{oneshot, watch};

use super::pod;
use crate::linux::publish;
use crate::linux::scale::{PixelLayout, SourceImage, downscale};

/// Puntos por lado que se promedian en cada celda del cuadro reducido.
const SAMPLES: u32 = 4;
/// `SPA_CHUNK_FLAG_CORRUPTED`.
const CHUNK_CORRUPTED: i32 = 1;

/// Por qué terminó el stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamEnd {
    /// Lo frenamos nosotros.
    Stopped,
    /// Se cortó del lado del compositor (monitor desconectado, sesión cerrada, error).
    Lost(String),
}

/// Para frenar el stream desde otro hilo.
pub(crate) struct StopHandle(mpsc::Sender<StreamEnd>);

impl StopHandle {
    pub(crate) fn stop(&self) {
        let _ = self.0.send(StreamEnd::Stopped);
    }
}

/// Arranca la lectura sobre la conexión a PipeWire que dio el portal.
pub(crate) fn spawn(
    fd: OwnedFd,
    node_id: u32,
    target: (u32, u32),
    interval: Duration,
    tx: Arc<watch::Sender<Option<Frame>>>,
) -> Result<(StopHandle, oneshot::Receiver<StreamEnd>, JoinHandle<()>), CaptureError> {
    let (events_tx, events_rx) = mpsc::channel();
    let (end_tx, end_rx) = oneshot::channel();
    let handle = StopHandle(events_tx.clone());
    let thread = std::thread::Builder::new()
        .name("openlight-pipewire".into())
        .spawn(move || {
            let end = match Pipeline::start(fd, node_id, target, interval, tx, events_tx) {
                Ok(pipeline) => {
                    let end = events_rx
                        .recv()
                        .unwrap_or(StreamEnd::Lost("canal cerrado".into()));
                    pipeline.shutdown();
                    end
                }
                Err(e) => StreamEnd::Lost(e.to_string()),
            };
            let _ = end_tx.send(end);
        })
        .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
    Ok((handle, end_rx, thread))
}

/// Estado que ven los callbacks (corren en el hilo del `pw_thread_loop`).
struct Callbacks {
    stream: *mut pw::pw_stream,
    format: Option<(u32, u32, PixelLayout)>,
    target: (u32, u32),
    min_gap: Duration,
    last: Option<Instant>,
    tx: Arc<watch::Sender<Option<Frame>>>,
    events: mpsc::Sender<StreamEnd>,
    streaming: bool,
    warned: bool,
}

/// Objetos de PipeWire vivos. Se destruyen en `shutdown`, en orden.
struct Pipeline {
    thread_loop: *mut pw::pw_thread_loop,
    context: *mut pw::pw_context,
    core: *mut pw::pw_core,
    stream: *mut pw::pw_stream,
    // Tienen que vivir mientras el stream exista: PipeWire guarda punteros a ellos.
    _hook: Box<spa::spa_hook>,
    _events: Box<pw::pw_stream_events>,
    callbacks: *mut Callbacks,
}

fn layout_of(format: u32) -> Option<PixelLayout> {
    use pod::video;
    Some(match format {
        video::BGRX | video::BGRA => PixelLayout::Bgrx,
        video::RGBX | video::RGBA => PixelLayout::Rgbx,
        video::XRGB | video::ARGB => PixelLayout::Xrgb,
        video::XBGR | video::ABGR => PixelLayout::Xbgr,
        _ => return None,
    })
}

impl Pipeline {
    fn start(
        fd: OwnedFd,
        node_id: u32,
        target: (u32, u32),
        interval: Duration,
        tx: Arc<watch::Sender<Option<Frame>>>,
        events: mpsc::Sender<StreamEnd>,
    ) -> Result<Self, CaptureError> {
        let fail = |what: &str| CaptureError::Backend(format!("PipeWire: falló {what}"));
        let fps = (1.0 / interval.as_secs_f64()).round() as u32;
        let format = pod::enum_format(fps);

        // SAFETY: llamadas a la API pública de libpipewire en el orden que documenta
        // (init → thread loop → context → core → stream). Cada puntero se verifica antes de
        // usarlo, y lo que se crea se destruye en `shutdown` o en `abort` si algo falla.
        unsafe {
            pw::pw_init(ptr::null_mut(), ptr::null_mut());
            let thread_loop = pw::pw_thread_loop_new(c"openlight-pw".as_ptr(), ptr::null());
            if thread_loop.is_null() {
                return Err(fail("pw_thread_loop_new"));
            }
            let context =
                pw::pw_context_new(pw::pw_thread_loop_get_loop(thread_loop), ptr::null_mut(), 0);
            if context.is_null() {
                pw::pw_thread_loop_destroy(thread_loop);
                return Err(fail("pw_context_new"));
            }
            if pw::pw_thread_loop_start(thread_loop) < 0 {
                pw::pw_context_destroy(context);
                pw::pw_thread_loop_destroy(thread_loop);
                return Err(fail("pw_thread_loop_start"));
            }
            pw::pw_thread_loop_lock(thread_loop);
            let abort = |core: *mut pw::pw_core| {
                if !core.is_null() {
                    pw::pw_core_disconnect(core);
                }
                pw::pw_thread_loop_unlock(thread_loop);
                pw::pw_thread_loop_stop(thread_loop);
                pw::pw_context_destroy(context);
                pw::pw_thread_loop_destroy(thread_loop);
            };

            // La conexión pasa a ser de PipeWire, que la cierra al desconectar.
            let core = pw::pw_context_connect_fd(context, fd.into_raw_fd(), ptr::null_mut(), 0);
            if core.is_null() {
                abort(core);
                return Err(fail("pw_context_connect_fd"));
            }
            let props = pw::pw_properties_new_string(
                c"media.type=Video media.category=Capture media.role=Screen".as_ptr(),
            );
            let stream = pw::pw_stream_new(core, c"openlight-captura".as_ptr(), props);
            if stream.is_null() {
                abort(core);
                return Err(fail("pw_stream_new"));
            }

            let callbacks = Box::into_raw(Box::new(Callbacks {
                stream,
                format: None,
                target,
                min_gap: interval.mul_f32(0.9),
                last: None,
                tx,
                events,
                streaming: false,
                warned: false,
            }));
            let mut hook: Box<spa::spa_hook> = Box::new(std::mem::zeroed());
            let mut stream_events: Box<pw::pw_stream_events> = Box::new(std::mem::zeroed());
            stream_events.version = pw::PW_VERSION_STREAM_EVENTS;
            stream_events.state_changed = Some(on_state_changed);
            stream_events.param_changed = Some(on_param_changed);
            stream_events.process = Some(on_process);
            pw::pw_stream_add_listener(
                stream,
                &mut *hook,
                &*stream_events,
                callbacks.cast::<c_void>(),
            );

            let mut params = [format.as_ptr().cast::<spa::spa_pod>()];
            let flags = pw::pw_stream_flags_PW_STREAM_FLAG_AUTOCONNECT
                | pw::pw_stream_flags_PW_STREAM_FLAG_MAP_BUFFERS;
            let res = pw::pw_stream_connect(
                stream,
                spa::SPA_DIRECTION_INPUT,
                node_id,
                flags,
                params.as_mut_ptr(),
                params.len() as u32,
            );
            let pipeline = Self {
                thread_loop,
                context,
                core,
                stream,
                _hook: hook,
                _events: stream_events,
                callbacks,
            };
            pw::pw_thread_loop_unlock(thread_loop);
            if res < 0 {
                pipeline.shutdown();
                return Err(fail(&format!("pw_stream_connect ({res})")));
            }
            Ok(pipeline)
        }
    }

    fn shutdown(self) {
        // SAFETY: los punteros salen de `start` y siguen vivos; se destruyen una sola vez, con
        // el lock del loop tomado mientras se tocan objetos que usa su hilo, y los callbacks
        // se liberan recién cuando el hilo del loop ya terminó.
        unsafe {
            pw::pw_thread_loop_lock(self.thread_loop);
            pw::pw_stream_disconnect(self.stream);
            pw::pw_stream_destroy(self.stream);
            pw::pw_core_disconnect(self.core);
            pw::pw_thread_loop_unlock(self.thread_loop);
            pw::pw_thread_loop_stop(self.thread_loop);
            pw::pw_context_destroy(self.context);
            pw::pw_thread_loop_destroy(self.thread_loop);
            drop(Box::from_raw(self.callbacks));
        }
    }
}

/// # Safety
/// `data` es el `Callbacks` registrado en `pw_stream_add_listener`, vivo hasta `shutdown`.
unsafe fn callbacks<'a>(data: *mut c_void) -> Option<&'a mut Callbacks> {
    // SAFETY: lo garantiza quien llama (ver arriba).
    unsafe { data.cast::<Callbacks>().as_mut() }
}

unsafe extern "C" fn on_state_changed(
    data: *mut c_void,
    _old: pw::pw_stream_state,
    state: pw::pw_stream_state,
    error: *const c_char,
) {
    // SAFETY: PipeWire pasa el `data` registrado; `error` es NULL o un string C válido.
    let Some(cb) = (unsafe { callbacks(data) }) else {
        return;
    };
    let message = if error.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    tracing::debug!("stream de PipeWire: estado {state} {message}");
    if state == pw::pw_stream_state_PW_STREAM_STATE_ERROR {
        let _ = cb
            .events
            .send(StreamEnd::Lost(format!("error de PipeWire: {message}")));
    } else if state == pw::pw_stream_state_PW_STREAM_STATE_UNCONNECTED {
        let _ = cb
            .events
            .send(StreamEnd::Lost("stream desconectado".into()));
    } else if state == pw::pw_stream_state_PW_STREAM_STATE_STREAMING {
        cb.streaming = true;
    } else if state == pw::pw_stream_state_PW_STREAM_STATE_PAUSED && cb.streaming {
        // El compositor pausa el stream cuando no hay nada que mostrar.
        cb.streaming = false;
        publish(&cb.tx, None);
    }
}

unsafe extern "C" fn on_param_changed(data: *mut c_void, id: u32, param: *const spa::spa_pod) {
    // SAFETY: PipeWire pasa el `data` registrado.
    let Some(cb) = (unsafe { callbacks(data) }) else {
        return;
    };
    if param.is_null() || id != pod::param::FORMAT {
        return;
    }
    // SAFETY: un `spa_pod` válido: encabezado de 8 bytes seguido de `size` bytes de cuerpo.
    let bytes = unsafe {
        let size = (*param).size as usize;
        std::slice::from_raw_parts(param.cast::<u8>(), 8 + size)
    };
    let Some(info) = pod::parse_video_format(bytes) else {
        return;
    };
    cb.format = layout_of(info.format).map(|l| (info.width, info.height, l));
    match cb.format {
        Some(_) => tracing::info!(
            "PipeWire negoció el formato {} de {}×{}",
            info.format,
            info.width,
            info.height
        ),
        None => tracing::warn!("PipeWire negoció un formato no soportado: {}", info.format),
    }
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    // SAFETY: PipeWire pasa el `data` registrado.
    let Some(cb) = (unsafe { callbacks(data) }) else {
        return;
    };
    // SAFETY: `cb.stream` es el stream vivo de este listener.
    let buffer = unsafe { pw::pw_stream_dequeue_buffer(cb.stream) };
    if buffer.is_null() {
        return;
    }
    // SAFETY: `buffer` lo acaba de dar PipeWire y se devuelve con `pw_stream_queue_buffer`
    // antes de salir; mientras tanto sus datos mapeados (MAP_BUFFERS) son válidos.
    unsafe {
        handle_buffer(cb, buffer);
        pw::pw_stream_queue_buffer(cb.stream, buffer);
    }
}

/// # Safety
/// `buffer` tiene que ser un búfer sacado de `cb.stream` y todavía no devuelto.
unsafe fn handle_buffer(cb: &mut Callbacks, buffer: *mut pw::pw_buffer) {
    let now = Instant::now();
    if cb.last.is_some_and(|last| now - last < cb.min_gap) {
        return;
    }
    let Some((width, height, layout)) = cb.format else {
        return;
    };
    // SAFETY: estructuras de SPA válidas mientras el búfer no se devuelva (ver arriba).
    let (bytes, offset, stride) = unsafe {
        let spa_buffer = (*buffer).buffer;
        if spa_buffer.is_null() || (*spa_buffer).n_datas == 0 || (*spa_buffer).datas.is_null() {
            return;
        }
        let data = &*(*spa_buffer).datas;
        if data.data.is_null() || data.chunk.is_null() {
            if !cb.warned {
                cb.warned = true;
                tracing::warn!("PipeWire entregó un búfer que no se puede mapear (¿DMA-BUF?)");
            }
            return;
        }
        let chunk = &*data.chunk;
        if chunk.flags & CHUNK_CORRUPTED != 0 {
            return;
        }
        let bytes = std::slice::from_raw_parts(data.data.cast::<u8>(), data.maxsize as usize);
        (bytes, chunk.offset as usize, chunk.stride)
    };
    let stride = if stride > 0 {
        stride as usize
    } else {
        width as usize * 4
    };
    let Some(bytes) = bytes.get(offset..) else {
        return;
    };
    let src = SourceImage {
        data: bytes,
        width,
        height,
        stride,
        layout,
    };
    match downscale(&src, cb.target, SAMPLES) {
        Ok(bgra) => {
            publish(&cb.tx, Frame::new(cb.target.0, cb.target.1, bgra).ok());
            cb.last = Some(now);
        }
        Err(e) => tracing::debug!("cuadro de PipeWire descartado: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las constantes de `pod.rs` tienen que coincidir con las de los headers instalados.
    #[test]
    fn constantes_del_pod_coinciden_con_spa() {
        assert_eq!(pod::ty::ID, spa::SPA_TYPE_Id);
        assert_eq!(pod::ty::RECTANGLE, spa::SPA_TYPE_Rectangle);
        assert_eq!(pod::ty::FRACTION, spa::SPA_TYPE_Fraction);
        assert_eq!(pod::ty::OBJECT, spa::SPA_TYPE_Object);
        assert_eq!(pod::ty::CHOICE, spa::SPA_TYPE_Choice);
        assert_eq!(pod::ty::OBJECT_FORMAT, spa::SPA_TYPE_OBJECT_Format);
        assert_eq!(pod::param::ENUM_FORMAT, spa::SPA_PARAM_EnumFormat);
        assert_eq!(pod::param::FORMAT, spa::SPA_PARAM_Format);
        assert_eq!(pod::key::MEDIA_TYPE, spa::SPA_FORMAT_mediaType);
        assert_eq!(pod::key::MEDIA_SUBTYPE, spa::SPA_FORMAT_mediaSubtype);
        assert_eq!(pod::key::VIDEO_FORMAT, spa::SPA_FORMAT_VIDEO_format);
        assert_eq!(pod::key::VIDEO_SIZE, spa::SPA_FORMAT_VIDEO_size);
        assert_eq!(pod::key::VIDEO_FRAMERATE, spa::SPA_FORMAT_VIDEO_framerate);
        assert_eq!(
            pod::key::VIDEO_MAX_FRAMERATE,
            spa::SPA_FORMAT_VIDEO_maxFramerate
        );
        assert_eq!(pod::MEDIA_TYPE_VIDEO, spa::SPA_MEDIA_TYPE_video);
        assert_eq!(pod::MEDIA_SUBTYPE_RAW, spa::SPA_MEDIA_SUBTYPE_raw);
        assert_eq!(pod::choice::NONE, spa::SPA_CHOICE_None);
        assert_eq!(pod::choice::RANGE, spa::SPA_CHOICE_Range);
        assert_eq!(pod::choice::ENUM, spa::SPA_CHOICE_Enum);
        assert_eq!(pod::video::RGBX, spa::SPA_VIDEO_FORMAT_RGBx);
        assert_eq!(pod::video::BGRX, spa::SPA_VIDEO_FORMAT_BGRx);
        assert_eq!(pod::video::XRGB, spa::SPA_VIDEO_FORMAT_xRGB);
        assert_eq!(pod::video::XBGR, spa::SPA_VIDEO_FORMAT_xBGR);
        assert_eq!(pod::video::RGBA, spa::SPA_VIDEO_FORMAT_RGBA);
        assert_eq!(pod::video::BGRA, spa::SPA_VIDEO_FORMAT_BGRA);
        assert_eq!(pod::video::ARGB, spa::SPA_VIDEO_FORMAT_ARGB);
        assert_eq!(pod::video::ABGR, spa::SPA_VIDEO_FORMAT_ABGR);
    }

    #[test]
    fn formatos_soportados() {
        assert_eq!(layout_of(pod::video::BGRX), Some(PixelLayout::Bgrx));
        assert_eq!(layout_of(pod::video::RGBA), Some(PixelLayout::Rgbx));
        assert_eq!(layout_of(pod::video::XBGR), Some(PixelLayout::Xbgr));
        assert_eq!(layout_of(2), None); // I420
    }

    /// Libera los objetos aunque no haya servidor: el connect falla con un fd que no es de
    /// PipeWire y no tiene que colgarse ni romper nada.
    #[test]
    fn falla_limpio_con_una_conexion_invalida() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        let (tx, _rx) = watch::channel(None);
        let (handle, end_rx, thread) = spawn(
            OwnedFd::from(a),
            1,
            (96, 54),
            Duration::from_millis(50),
            Arc::new(tx),
        )
        .unwrap();
        // Si el connect no falla enseguida, se frena a mano.
        std::thread::sleep(Duration::from_millis(200));
        handle.stop();
        thread.join().unwrap();
        assert!(end_rx.blocking_recv().is_ok());
    }
}
