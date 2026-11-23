//! Backend X11 (también XWayland).
//!
//! - Monitores: RandR 1.5 (`GetMonitors`); si no está, la pantalla entera como un solo monitor.
//! - Cuadro: XRender reduce el monitor en el servidor (transformación de escala + filtro
//!   bilineal) a un pixmap de 4× el tamaño pedido, y acá se promedian bloques de 4×4. Así solo
//!   viajan unos 330 KB por cuadro (a 96×54) y no hace falta copiar la pantalla entera: por eso
//!   no se usa XShm, que solo ahorra la copia por el socket de imágenes grandes.
//! - Pantalla apagada (DPMS) o monitor que no está: se publica `None` y se sigue revisando.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use openlight_types::{CaptureError, DisplayId, DisplayInfo, Frame};
use tokio::sync::watch;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::dpms::{self, ConnectionExt as _};
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::render::{self, ConnectionExt as _, CreatePictureAux, PictOp, Transform};
use x11rb::protocol::xproto::{
    ConnectionExt as _, ImageFormat, ImageOrder, Pixmap, SubwindowMode, Window,
};
use x11rb::rust_connection::RustConnection;

use super::detect::looks_builtin;
use super::scale::{PixelLayout, SourceImage, downscale};
use super::{frame_interval, publish, sleep_until};

/// Factor de sobremuestreo: el servidor reduce a `objetivo × SUPERSAMPLE` y acá se promedia.
const SUPERSAMPLE: u32 = 4;

/// Parámetros de conexión del backend.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// Nombre del display (`:0`). `None` usa `DISPLAY`.
    pub display_name: Option<String>,
    /// Cada cuánto se vuelve a mirar qué monitores hay (en `ambilight.py`, `REVISAR_CADA = 3`).
    pub recheck: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            display_name: None,
            recheck: Duration::from_secs(3),
        }
    }
}

fn backend_err(context: &str) -> impl FnOnce(&dyn std::fmt::Display) -> CaptureError + '_ {
    move |e| CaptureError::Backend(format!("X11, {context}: {e}"))
}

macro_rules! x11 {
    ($context:expr, $e:expr) => {
        $e.map_err(|e| backend_err($context)(&e))
    };
}

/// Un monitor de X11 con su posición en la pantalla virtual.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Monitor {
    info: DisplayInfo,
    x: i16,
    y: i16,
}

/// Monitores activos del display.
pub(crate) fn displays(opts: &Options) -> Result<Vec<DisplayInfo>, CaptureError> {
    let (conn, screen) = connect(opts)?;
    Ok(list_monitors(&conn, screen)?
        .into_iter()
        .map(|m| m.info)
        .collect())
}

fn connect(opts: &Options) -> Result<(RustConnection, usize), CaptureError> {
    RustConnection::connect(opts.display_name.as_deref())
        .map_err(|e| CaptureError::Unsupported(format!("no se pudo abrir el display X11: {e}")))
}

fn list_monitors(conn: &RustConnection, screen: usize) -> Result<Vec<Monitor>, CaptureError> {
    let root = &conn.setup().roots[screen];
    if x11!(
        "RandR",
        conn.extension_information(randr::X11_EXTENSION_NAME)
    )?
    .is_some()
    {
        // GetMonitors es de RandR 1.5; con un servidor más viejo responde error y se usa
        // la pantalla entera.
        let version = x11!("RandR", conn.randr_query_version(1, 5))?.reply();
        let has_monitors = version.is_ok_and(|v| (v.major_version, v.minor_version) >= (1, 5));
        if has_monitors {
            let reply = x11!("GetMonitors", conn.randr_get_monitors(root.root, true))?;
            let reply = x11!("GetMonitors", reply.reply())?;
            let mut monitors = Vec::with_capacity(reply.monitors.len());
            for m in reply.monitors {
                let name = x11!("GetAtomName", conn.get_atom_name(m.name))?;
                let name = x11!("GetAtomName", name.reply())?;
                let name = String::from_utf8_lossy(&name.name).into_owned();
                monitors.push(Monitor {
                    info: DisplayInfo {
                        id: DisplayId(name.clone()),
                        builtin: looks_builtin(&name),
                        name,
                        width: u32::from(m.width),
                        height: u32::from(m.height),
                        primary: m.primary,
                    },
                    x: m.x,
                    y: m.y,
                });
            }
            if !monitors.is_empty() {
                if !monitors.iter().any(|m| m.info.primary) {
                    monitors[0].info.primary = true;
                }
                return Ok(monitors);
            }
        }
    }
    let name = format!("SCREEN-{screen}");
    Ok(vec![Monitor {
        info: DisplayInfo {
            id: DisplayId(name.clone()),
            name,
            width: u32::from(root.width_in_pixels),
            height: u32::from(root.height_in_pixels),
            builtin: false,
            primary: true,
        },
        x: 0,
        y: 0,
    }])
}

/// Pixmap destino donde XRender deja el monitor reducido.
struct Canvas {
    pixmap: Pixmap,
    picture: render::Picture,
    size: (u16, u16),
    /// Geometría del monitor para la que está armada la transformación.
    geometry: (i16, i16, u16, u16),
}

/// Conexión lista para capturar: picture de la raíz con XRender y formato de los píxeles.
struct Grabber {
    conn: RustConnection,
    screen: usize,
    root: Window,
    depth: u8,
    format: render::Pictformat,
    layout: PixelLayout,
    source: render::Picture,
    dpms: bool,
    canvas: Option<Canvas>,
}

impl Grabber {
    fn connect(opts: &Options) -> Result<Self, CaptureError> {
        let (conn, screen) = connect(opts)?;
        if x11!(
            "RENDER",
            conn.extension_information(render::X11_EXTENSION_NAME)
        )?
        .is_none()
        {
            return Err(CaptureError::Unsupported(
                "el servidor X11 no tiene la extensión RENDER".into(),
            ));
        }
        x11!(
            "RENDER",
            x11!("RENDER", conn.render_query_version(0, 11))?.reply()
        )?;

        let setup = conn.setup();
        let root = &setup.roots[screen];
        let (root_window, depth, visual) = (root.root, root.root_depth, root.root_visual);
        let bpp = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == depth)
            .map(|f| f.bits_per_pixel);
        if bpp != Some(32) {
            return Err(CaptureError::Unsupported(format!(
                "profundidad de color {depth} con {bpp:?} bits por píxel (se espera 32)"
            )));
        }
        let big_endian = setup.image_byte_order == ImageOrder::MSB_FIRST;

        let formats = x11!("QueryPictFormats", conn.render_query_pict_formats())?;
        let formats = x11!("QueryPictFormats", formats.reply())?;
        let format = formats
            .screens
            .get(screen)
            .and_then(|s| s.depths.iter().find(|d| d.depth == depth))
            .and_then(|d| d.visuals.iter().find(|v| v.visual == visual))
            .map(|v| v.format)
            .ok_or_else(|| CaptureError::Unsupported("RENDER no conoce el visual raíz".into()))?;
        let direct = formats
            .formats
            .iter()
            .find(|f| f.id == format)
            .map(|f| f.direct)
            .ok_or_else(|| CaptureError::Unsupported("formato RENDER desconocido".into()))?;
        let mask = |m: u16, shift: u16| u32::from(m) << shift;
        let layout = PixelLayout::from_masks(
            mask(direct.red_mask, direct.red_shift),
            mask(direct.green_mask, direct.green_shift),
            mask(direct.blue_mask, direct.blue_shift),
            big_endian,
        )
        .ok_or_else(|| CaptureError::Unsupported(format!("máscaras de color raras: {direct:?}")))?;

        let source = x11!("generate_id", conn.generate_id())?;
        x11!(
            "CreatePicture",
            conn.render_create_picture(
                source,
                root_window,
                format,
                &CreatePictureAux::new().subwindowmode(SubwindowMode::INCLUDE_INFERIORS),
            )
        )?;
        x11!(
            "SetPictureFilter",
            conn.render_set_picture_filter(source, b"bilinear", &[])
        )?;

        let dpms = conn
            .extension_information(dpms::X11_EXTENSION_NAME)
            .ok()
            .flatten()
            .is_some()
            && conn
                .dpms_capable()
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|r| r.capable);

        Ok(Self {
            conn,
            screen,
            root: root_window,
            depth,
            format,
            layout,
            source,
            dpms,
            canvas: None,
        })
    }

    /// `false` si DPMS dice que la pantalla está apagada o en reposo.
    fn screen_is_on(&self) -> bool {
        if !self.dpms {
            return true;
        }
        match self.conn.dpms_info().ok().and_then(|c| c.reply().ok()) {
            Some(info) => !info.state || info.power_level == dpms::DPMSMode::ON,
            None => true,
        }
    }

    fn ensure_canvas(&mut self, mon: &Monitor, target: (u32, u32)) -> Result<(), CaptureError> {
        let size = canvas_size(mon, target);
        let geometry = (mon.x, mon.y, mon.info.width as u16, mon.info.height as u16);
        if let Some(c) = &self.canvas
            && c.size == size
            && c.geometry == geometry
        {
            return Ok(());
        }
        if let Some(old) = self.canvas.take() {
            let _ = self.conn.render_free_picture(old.picture);
            let _ = self.conn.free_pixmap(old.pixmap);
        }
        let pixmap = x11!("generate_id", self.conn.generate_id())?;
        x11!(
            "CreatePixmap",
            self.conn
                .create_pixmap(self.depth, pixmap, self.root, size.0, size.1)
        )?;
        let picture = x11!("generate_id", self.conn.generate_id())?;
        x11!(
            "CreatePicture",
            self.conn
                .render_create_picture(picture, pixmap, self.format, &CreatePictureAux::new())
        )?;
        x11!(
            "SetPictureTransform",
            self.conn
                .render_set_picture_transform(self.source, scale_transform(mon, size))
        )?;
        self.canvas = Some(Canvas {
            pixmap,
            picture,
            size,
            geometry,
        });
        Ok(())
    }

    /// Captura el monitor reducido a `target`, en BGRA.
    fn grab(&mut self, mon: &Monitor, target: (u32, u32)) -> Result<Vec<u8>, CaptureError> {
        self.ensure_canvas(mon, target)?;
        let canvas = self.canvas.as_ref().expect("recién creado");
        let (w, h) = canvas.size;
        x11!(
            "Composite",
            self.conn.render_composite(
                PictOp::SRC,
                self.source,
                x11rb::NONE,
                canvas.picture,
                0,
                0,
                0,
                0,
                0,
                0,
                w,
                h,
            )
        )?;
        let image = x11!(
            "GetImage",
            self.conn
                .get_image(ImageFormat::Z_PIXMAP, canvas.pixmap, 0, 0, w, h, !0)
        )?;
        let image = x11!("GetImage", image.reply())?;
        let src = SourceImage {
            data: &image.data,
            width: u32::from(w),
            height: u32::from(h),
            stride: usize::from(w) * 4,
            layout: self.layout,
        };
        downscale(&src, target, SUPERSAMPLE).map_err(|e| backend_err("reducir")(&e))
    }
}

/// Tamaño del pixmap intermedio: `objetivo × SUPERSAMPLE`, sin pasarse del monitor.
fn canvas_size(mon: &Monitor, target: (u32, u32)) -> (u16, u16) {
    let side = |t: u32, m: u32| (t.saturating_mul(SUPERSAMPLE)).min(m).clamp(1, 4096) as u16;
    (
        side(target.0, mon.info.width),
        side(target.1, mon.info.height),
    )
}

/// Transformación de XRender que lleva cada píxel del pixmap al punto del monitor que le toca.
///
/// RENDER aplica la matriz a coordenadas de destino para obtener las de origen; la traslación
/// ubica el monitor dentro de la pantalla virtual. Valores en punto fijo 16.16.
fn scale_transform(mon: &Monitor, size: (u16, u16)) -> Transform {
    let fixed = |v: f64| (v * 65536.0).round() as i32;
    Transform {
        matrix11: fixed(f64::from(mon.info.width) / f64::from(size.0)),
        matrix12: 0,
        matrix13: fixed(f64::from(mon.x)),
        matrix21: 0,
        matrix22: fixed(f64::from(mon.info.height) / f64::from(size.1)),
        matrix23: fixed(f64::from(mon.y)),
        matrix31: 0,
        matrix32: 0,
        matrix33: fixed(1.0),
    }
}

/// Captura en curso: un hilo que publica en un `watch`.
pub(crate) struct X11Capture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl X11Capture {
    /// Abre la conexión, verifica que el monitor exista y arranca el hilo.
    pub(crate) fn start(
        opts: Options,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> Result<(Self, watch::Receiver<Option<Frame>>), CaptureError> {
        let interval = frame_interval(fps)?;
        let grabber = Grabber::connect(&opts)?;
        let monitor = list_monitors(&grabber.conn, grabber.screen)?
            .into_iter()
            .find(|m| m.info.id == display)
            .ok_or_else(|| CaptureError::DisplayNotFound(display.clone()))?;
        let (tx, rx) = watch::channel(None);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("openlight-x11".into())
            .spawn({
                let stop = Arc::clone(&stop);
                move || {
                    run(
                        opts,
                        display,
                        target,
                        interval,
                        tx,
                        &stop,
                        Some(grabber),
                        Some(monitor),
                    );
                }
            })
            .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
        Ok((
            Self {
                stop,
                thread: Some(thread),
            },
            rx,
        ))
    }

    /// Frena el hilo y espera que termine.
    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    opts: Options,
    wanted: DisplayId,
    target: (u32, u32),
    interval: Duration,
    tx: watch::Sender<Option<Frame>>,
    stop: &AtomicBool,
    mut grabber: Option<Grabber>,
    mut monitor: Option<Monitor>,
) {
    let mut next_check = Instant::now() + opts.recheck;
    let mut next_frame = Instant::now();
    let mut was_present = true;
    while !stop.load(Ordering::Relaxed) && !tx.is_closed() {
        let now = Instant::now();
        if now >= next_check {
            next_check = now + opts.recheck;
            if grabber.is_none() {
                grabber = Grabber::connect(&opts)
                    .map_err(|e| tracing::debug!("X11 sigue sin conexión: {e}"))
                    .ok();
            }
            monitor = match &grabber {
                Some(g) => match list_monitors(&g.conn, g.screen) {
                    Ok(list) => list.into_iter().find(|m| m.info.id == wanted),
                    Err(e) => {
                        tracing::warn!("se cortó la conexión con X11: {e}");
                        grabber = None;
                        None
                    }
                },
                None => None,
            };
            let present = monitor.is_some();
            if present != was_present {
                if present {
                    tracing::info!("volvió el monitor {}", wanted);
                } else {
                    tracing::info!("el monitor {} no está; esperando que aparezca", wanted);
                }
                was_present = present;
            }
        }

        let frame = match (grabber.as_mut(), monitor.as_ref()) {
            (Some(g), Some(m)) if g.screen_is_on() => match g.grab(m, target) {
                Ok(bgra) => Frame::new(target.0, target.1, bgra)
                    .map_err(|e| tracing::warn!("cuadro inválido: {e}"))
                    .ok(),
                Err(e) => {
                    tracing::warn!("falló la captura X11, reconectando: {e}");
                    grabber = None;
                    monitor = None;
                    next_check = now + opts.recheck;
                    None
                }
            },
            _ => None,
        };
        publish(&tx, frame);

        next_frame += interval;
        if next_frame < now {
            next_frame = now + interval;
        }
        sleep_until(next_frame.min(next_check), stop);
    }
}

#[cfg(test)]
mod tests {
    //! Pruebas contra un Xvfb de verdad. Si `Xvfb` no está instalado, se saltean con un aviso
    //! (en el CI de Linux se instala con el paquete `xvfb`).

    use super::*;
    use std::process::{Child, Command, Stdio};
    use x11rb::protocol::xproto::{CreateGCAux, Rectangle};
    use x11rb::wrapper::ConnectionExt as _;

    struct Xvfb {
        child: Child,
        name: String,
    }

    impl Xvfb {
        fn start() -> Option<Self> {
            let available = Command::new("sh")
                .args(["-c", "command -v Xvfb"])
                .stdout(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !available {
                eprintln!("Xvfb no está instalado: se saltea la prueba de X11");
                return None;
            }
            // Un número de display por proceso y por prueba, para no chocar en paralelo.
            // `-noreset`: si no, Xvfb se reinicia cuando se va el último cliente (el sondeo de
            // abajo) y la conexión siguiente recibe "connection reset by peer".
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = 90 + (std::process::id() % 400) * 8 + NEXT.fetch_add(1, Ordering::Relaxed);
            let name = format!(":{n}");
            let child = Command::new("Xvfb")
                .args([
                    &name,
                    "-screen",
                    "0",
                    "200x100x24",
                    "-nolisten",
                    "tcp",
                    "-noreset",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("arrancar Xvfb");
            let deadline = Instant::now() + Duration::from_secs(10);
            while RustConnection::connect(Some(&name)).is_err() {
                assert!(Instant::now() < deadline, "Xvfb no arrancó");
                std::thread::sleep(Duration::from_millis(50));
            }
            Some(Self { child, name })
        }

        fn options(&self) -> Options {
            Options {
                display_name: Some(self.name.clone()),
                recheck: Duration::from_millis(100),
            }
        }
    }

    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Pinta la raíz: mitad izquierda roja, mitad derecha azul.
    fn paint(conn: &RustConnection, screen: usize) {
        let root = conn.setup().roots[screen].root;
        for (color, x) in [(0x00ff_0000u32, 0i16), (0x0000_00ff, 100)] {
            let gc = conn.generate_id().unwrap();
            conn.create_gc(gc, root, &CreateGCAux::new().foreground(color))
                .unwrap();
            conn.poly_fill_rectangle(
                root,
                gc,
                &[Rectangle {
                    x,
                    y: 0,
                    width: 100,
                    height: 100,
                }],
            )
            .unwrap();
        }
        conn.sync().unwrap();
    }

    fn set_monitor(conn: &RustConnection, screen: usize, name: &str, x: i16) {
        let root = conn.setup().roots[screen].root;
        let atom = conn
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        conn.randr_set_monitor(
            root,
            randr::MonitorInfo {
                name: atom,
                primary: x == 0,
                automatic: false,
                x,
                y: 0,
                width: 100,
                height: 100,
                width_in_millimeters: 0,
                height_in_millimeters: 0,
                outputs: vec![],
            },
        )
        .unwrap();
        conn.sync().unwrap();
    }

    fn delete_monitor(conn: &RustConnection, screen: usize, name: &str) {
        let root = conn.setup().roots[screen].root;
        let atom = conn
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        conn.randr_delete_monitor(root, atom).unwrap();
        conn.sync().unwrap();
    }

    fn wait_frame(
        rx: &mut watch::Receiver<Option<Frame>>,
        pred: impl Fn(&Option<Frame>) -> bool,
    ) -> Option<Frame> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let current = rx.borrow_and_update().clone();
            if pred(&current) {
                return current;
            }
            assert!(Instant::now() < deadline, "no llegó el cuadro esperado");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Color promedio `(r, g, b)` de un cuadro BGRA.
    fn average(frame: &Frame) -> (u8, u8, u8) {
        let n = (frame.width * frame.height) as u64;
        let mut sum = [0u64; 3];
        for px in frame.bgra.as_chunks::<4>().0 {
            sum[0] += u64::from(px[2]);
            sum[1] += u64::from(px[1]);
            sum[2] += u64::from(px[0]);
        }
        let avg = |s: u64| (s / n) as u8;
        (avg(sum[0]), avg(sum[1]), avg(sum[2]))
    }

    #[test]
    fn lista_la_pantalla_de_xvfb() {
        let Some(x) = Xvfb::start() else { return };
        let list = displays(&x.options()).unwrap();
        assert!(!list.is_empty());
        assert!(list.iter().any(|d| d.primary));
        let total: u32 = list.iter().map(|d| d.width).max().unwrap();
        assert_eq!(total, 200);
    }

    #[test]
    fn captura_cada_monitor_y_reacciona_cuando_se_va_y_vuelve() {
        let Some(x) = Xvfb::start() else { return };
        let (conn, screen) = RustConnection::connect(Some(&x.name)).unwrap();
        paint(&conn, screen);
        set_monitor(&conn, screen, "IZQ-1", 0);
        set_monitor(&conn, screen, "DER-1", 100);

        let ids: Vec<_> = displays(&x.options())
            .unwrap()
            .into_iter()
            .map(|d| d.id.0)
            .collect();
        assert!(ids.contains(&"IZQ-1".to_string()), "{ids:?}");
        assert!(ids.contains(&"DER-1".to_string()), "{ids:?}");

        let (mut izq, mut rx_izq) =
            X11Capture::start(x.options(), DisplayId("IZQ-1".into()), (96, 54), 30.0).unwrap();
        let (mut der, mut rx_der) =
            X11Capture::start(x.options(), DisplayId("DER-1".into()), (96, 54), 30.0).unwrap();

        let frame = wait_frame(&mut rx_izq, Option::is_some).unwrap();
        assert_eq!((frame.width, frame.height), (96, 54));
        assert_eq!(frame.bgra.len(), 96 * 54 * 4);
        assert_eq!(average(&frame), (255, 0, 0));
        let frame = wait_frame(&mut rx_der, Option::is_some).unwrap();
        assert_eq!(average(&frame), (0, 0, 255));

        // El monitor se va: llega None. Vuelve: llegan cuadros otra vez.
        delete_monitor(&conn, screen, "DER-1");
        wait_frame(&mut rx_der, Option::is_none);
        set_monitor(&conn, screen, "DER-1", 100);
        let frame = wait_frame(&mut rx_der, Option::is_some).unwrap();
        assert_eq!(average(&frame), (0, 0, 255));

        izq.stop();
        der.stop();
    }

    #[test]
    fn monitor_desconocido() {
        let Some(x) = Xvfb::start() else { return };
        let out = X11Capture::start(x.options(), DisplayId("NO-EXISTE".into()), (96, 54), 20.0);
        assert!(matches!(out, Err(CaptureError::DisplayNotFound(_))));
    }

    #[test]
    fn la_transformacion_ubica_el_monitor() {
        let mon = Monitor {
            info: DisplayInfo {
                id: DisplayId("DP-1".into()),
                name: "DP-1".into(),
                width: 1920,
                height: 1080,
                builtin: false,
                primary: false,
            },
            x: 2560,
            y: 0,
        };
        let size = canvas_size(&mon, (96, 54));
        assert_eq!(size, (384, 216));
        let t = scale_transform(&mon, size);
        assert_eq!(t.matrix11, 5 << 16);
        assert_eq!(t.matrix22, 5 << 16);
        assert_eq!(t.matrix13, 2560 << 16);
        assert_eq!(t.matrix33, 1 << 16);
    }
}
