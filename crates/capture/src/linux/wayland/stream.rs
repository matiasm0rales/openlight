//! Lectura de cuadros por PipeWire, en un hilo propio con su `MainLoop`.
//!
//! Se pide video crudo de 32 bits (BGRx, RGBx y variantes) en memoria compartida: no se ofrecen
//! modificadores DMA-BUF, así el compositor entrega búferes mapeables por la CPU. Cada cuadro se
//! reduce con una grilla rala de puntos ([`downscale`]) y se publica a lo sumo a `fps`.

use std::cell::RefCell;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use openlight_types::{CaptureError, Frame};
use pipewire as pw;
use pw::spa;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::pod::Pod;
use pw::stream::{StreamFlags, StreamState};
use tokio::sync::{oneshot, watch};

use crate::linux::publish;
use crate::linux::scale::{PixelLayout, SourceImage, downscale};

/// Puntos por lado que se promedian en cada celda del cuadro reducido.
const SAMPLES: u32 = 4;

/// Por qué terminó el stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamEnd {
    /// Lo frenamos nosotros.
    Stopped,
    /// Se cortó del lado del compositor (monitor desconectado, sesión cerrada, error).
    Lost(String),
}

/// Para frenar el hilo de PipeWire desde otro hilo.
pub(crate) struct StopHandle(pw::channel::Sender<()>);

impl StopHandle {
    pub(crate) fn stop(&self) {
        let _ = self.0.send(());
    }
}

/// Arranca el hilo de PipeWire sobre la conexión que dio el portal.
pub(crate) fn spawn(
    fd: OwnedFd,
    node_id: u32,
    target: (u32, u32),
    interval: Duration,
    tx: Arc<watch::Sender<Option<Frame>>>,
) -> Result<(StopHandle, oneshot::Receiver<StreamEnd>, JoinHandle<()>), CaptureError> {
    let (stop_tx, stop_rx) = pw::channel::channel::<()>();
    let (end_tx, end_rx) = oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("openlight-pipewire".into())
        .spawn(move || {
            let end = match run(fd, node_id, target, interval, &tx, stop_rx) {
                Ok(end) => end,
                Err(e) => StreamEnd::Lost(e.to_string()),
            };
            let _ = end_tx.send(end);
        })
        .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
    Ok((StopHandle(stop_tx), end_rx, thread))
}

/// Estado que ven los callbacks del stream.
struct Shared {
    format: Option<(u32, u32, PixelLayout)>,
    target: (u32, u32),
    min_gap: Duration,
    last: Option<Instant>,
    tx: Arc<watch::Sender<Option<Frame>>>,
    warned: bool,
}

fn layout_of(format: VideoFormat) -> Option<PixelLayout> {
    Some(match format {
        VideoFormat::BGRx | VideoFormat::BGRA => PixelLayout::Bgrx,
        VideoFormat::RGBx | VideoFormat::RGBA => PixelLayout::Rgbx,
        VideoFormat::xRGB | VideoFormat::ARGB => PixelLayout::Xrgb,
        VideoFormat::xBGR | VideoFormat::ABGR => PixelLayout::Xbgr,
        _ => return None,
    })
}

fn run(
    fd: OwnedFd,
    node_id: u32,
    target: (u32, u32),
    interval: Duration,
    tx: &Arc<watch::Sender<Option<Frame>>>,
    stop_rx: pw::channel::Receiver<()>,
) -> Result<StreamEnd, CaptureError> {
    let err = |what: &str, e: pw::Error| CaptureError::Backend(format!("PipeWire, {what}: {e}"));
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None).map_err(|e| err("main loop", e))?;
    let context = pw::context::ContextRc::new(&main_loop, None).map_err(|e| err("contexto", e))?;
    let core = context
        .connect_fd_rc(fd, None)
        .map_err(|e| err("conectar al portal", e))?;

    let end: Rc<RefCell<Option<StreamEnd>>> = Rc::new(RefCell::new(None));
    let finish = {
        let main_loop = main_loop.clone();
        let end = Rc::clone(&end);
        move |reason: StreamEnd| {
            end.borrow_mut().get_or_insert(reason);
            main_loop.quit();
        }
    };

    let _stop = stop_rx.attach(main_loop.loop_(), {
        let finish = finish.clone();
        move |()| finish(StreamEnd::Stopped)
    });
    let _core_listener = core
        .add_listener_local()
        .error({
            let finish = finish.clone();
            move |id, _seq, res, message| {
                if id == pw::core::PW_ID_CORE {
                    finish(StreamEnd::Lost(format!(
                        "error de PipeWire {res}: {message}"
                    )));
                }
            }
        })
        .register();

    let stream = pw::stream::StreamRc::new(
        core,
        "openlight-captura",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|e| err("stream", e))?;

    let shared = Shared {
        format: None,
        target,
        min_gap: interval.mul_f32(0.9),
        last: None,
        tx: Arc::clone(tx),
        warned: false,
    };
    let _listener = stream
        .add_local_listener_with_user_data(shared)
        .state_changed({
            let finish = finish.clone();
            move |_, shared, old, new| {
                tracing::debug!("stream de PipeWire: {old:?} → {new:?}");
                match new {
                    StreamState::Error(message) => finish(StreamEnd::Lost(message)),
                    StreamState::Unconnected => {
                        finish(StreamEnd::Lost("stream desconectado".into()))
                    }
                    // El compositor pausa el stream cuando no hay nada que mostrar.
                    StreamState::Paused if matches!(old, StreamState::Streaming) => {
                        publish(&shared.tx, None);
                    }
                    _ => {}
                }
            }
        })
        .param_changed(|_, shared, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param)
            else {
                return;
            };
            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }
            let mut info = VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }
            let size = info.size();
            shared.format = layout_of(info.format()).map(|l| (size.width, size.height, l));
            match shared.format {
                Some(f) => {
                    tracing::info!("PipeWire negoció {:?} de {}×{}", info.format(), f.0, f.1)
                }
                None => tracing::warn!(
                    "PipeWire negoció un formato no soportado: {:?}",
                    info.format()
                ),
            }
        })
        .process(|stream, shared| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let now = Instant::now();
            if shared.last.is_some_and(|last| now - last < shared.min_gap) {
                return;
            }
            let Some((width, height, layout)) = shared.format else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            if chunk.flags().contains(spa::buffer::ChunkFlags::CORRUPTED) {
                return;
            }
            let offset = chunk.offset() as usize;
            let stride = match chunk.stride() {
                s if s > 0 => s as usize,
                _ => width as usize * 4,
            };
            let Some(bytes) = data.data() else {
                if !shared.warned {
                    shared.warned = true;
                    tracing::warn!("PipeWire entregó un búfer que no se puede mapear (¿DMA-BUF?)");
                }
                return;
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
            match downscale(&src, shared.target, SAMPLES) {
                Ok(bgra) => {
                    let frame = Frame::new(shared.target.0, shared.target.1, bgra).ok();
                    publish(&shared.tx, frame);
                    shared.last = Some(now);
                }
                Err(e) => tracing::debug!("cuadro de PipeWire descartado: {e}"),
            }
        })
        .register()
        .map_err(|e| err("listener", e))?;

    let format = format_pod(interval)?;
    let mut params = [Pod::from_bytes(&format)
        .ok_or_else(|| CaptureError::Backend("POD de formato inválido".into()))?];
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node_id),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| err("connect", e))?;

    main_loop.run();
    let _ = stream.disconnect();
    let reason = end.borrow_mut().take().unwrap_or(StreamEnd::Stopped);
    Ok(reason)
}

/// `EnumFormat` que se ofrece: video crudo de 32 bits, cualquier tamaño, `fps` como máximo.
fn format_pod(interval: Duration) -> Result<Vec<u8>, CaptureError> {
    let fps = (1.0 / interval.as_secs_f64()).round().clamp(1.0, 1000.0) as u32;
    let fraction = |num| spa::utils::Fraction { num, denom: 1 };
    let object = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
            VideoFormat::xRGB,
            VideoFormat::ARGB,
            VideoFormat::xBGR,
            VideoFormat::ABGR,
        ),
        spa::pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        spa::pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            fraction(fps),
            spa::utils::Fraction { num: 0, denom: 1 },
            fraction(1000)
        ),
        spa::pod::property!(
            FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            fraction(fps),
            fraction(1),
            fraction(1000)
        ),
    );
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|e| CaptureError::Backend(format!("serializar el formato: {e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatos_soportados() {
        assert_eq!(layout_of(VideoFormat::BGRx), Some(PixelLayout::Bgrx));
        assert_eq!(layout_of(VideoFormat::RGBA), Some(PixelLayout::Rgbx));
        assert_eq!(layout_of(VideoFormat::xBGR), Some(PixelLayout::Xbgr));
        assert_eq!(layout_of(VideoFormat::I420), None);
    }

    #[test]
    fn el_pod_de_formato_se_serializa() {
        let bytes = format_pod(Duration::from_millis(50)).unwrap();
        assert!(Pod::from_bytes(&bytes).is_some());
    }
}
