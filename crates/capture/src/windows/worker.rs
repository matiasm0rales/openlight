//! Hilo de captura: una sesión de Windows Graphics Capture por monitor, consultada a `fps`.
//!
//! El frame pool es *free-threaded*: no necesita un `DispatcherQueue` ni un bucle de mensajes
//! de WinRT, y el hilo lo consulta con `TryGetNextFrame` en cada tic. Graphics Capture solo
//! entrega cuadros cuando cambia algo en pantalla; si no llega uno nuevo, el `watch` conserva
//! el último (la pantalla sigue mostrando eso).
//!
//! Cada [`MONITOR_CHECK`] el hilo vuelve a enumerar los monitores: si el pedido desaparece,
//! publica `None`; si vuelve, o cambia su modo (resolución, HDR), rearma la sesión. Si Windows
//! apaga las pantallas, publica `None` y suelta la sesión hasta que vuelvan.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ::windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind,
    GraphicsCaptureItem, GraphicsCaptureSession,
};
use ::windows::Graphics::DirectX::DirectXPixelFormat;
use ::windows::Graphics::SizeInt32;
use ::windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use ::windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use ::windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use ::windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use ::windows::core::{Interface, factory};
use openlight_types::{CaptureError, DisplayId, Frame};
use tokio::sync::watch;

use super::gpu::{Downscaler, Gpu, PixelFormat};
use super::monitors::{self, Monitor};
use super::power::DisplayPower;

/// Cada cuánto se revisa qué monitores hay (como `Captura.REVISAR_CADA` del script, que usa 3 s).
const MONITOR_CHECK: Duration = Duration::from_secs(2);
/// Espera antes de reintentar una sesión que falló.
const RETRY_AFTER: Duration = Duration::from_secs(1);
/// Cuadros en el frame pool. Con 2, el más nuevo tiene como mucho un tic de antigüedad.
const POOL_BUFFERS: i32 = 2;

/// Pedido de captura que recibe el hilo.
pub(crate) struct Request {
    pub display: DisplayId,
    pub target: (u32, u32),
    pub interval: Duration,
}

/// Hilo de captura en marcha. Al soltarlo, se detiene y se espera a que termine.
pub(crate) struct Worker {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Lanza el hilo y espera a que arme la primera sesión, para devolver los errores de
    /// arranque (Graphics Capture no soportado, sin GPU) como errores de `start`.
    pub fn spawn(request: Request, tx: watch::Sender<Option<Frame>>) -> Result<Self, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let stop_hilo = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name(format!("openlight-captura-{}", request.display))
            .spawn(move || run(request, tx, stop_hilo, ready_tx))
            .map_err(|e| CaptureError::Backend(format!("no se pudo crear el hilo: {e}")))?;
        let mut worker = Self {
            stop,
            handle: Some(handle),
        };
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(worker),
            Ok(Err(e)) => {
                worker.shutdown();
                Err(e)
            }
            Err(_) => {
                worker.shutdown();
                Err(CaptureError::Backend(
                    "el hilo de captura terminó al arrancar".into(),
                ))
            }
        }
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Sesión de captura de un monitor.
struct Session {
    monitor: Monitor,
    format: PixelFormat,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    pool_size: SizeInt32,
    downscaler: Option<Downscaler>,
}

impl Session {
    fn open(gpu: &Gpu, monitor: Monitor) -> ::windows::core::Result<Self> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: `hmonitor` salió de EnumDisplayMonitors recién; si ya no existe, la llamada
        // devuelve un error en vez de fallar.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor.hmonitor)? };
        let format = if monitor.advanced_color {
            PixelFormat::Rgba16f
        } else {
            PixelFormat::Bgra8
        };
        let pixel_format = match format {
            PixelFormat::Bgra8 => DirectXPixelFormat::B8G8R8A8UIntNormalized,
            PixelFormat::Rgba16f => DirectXPixelFormat::R16G16B16A16Float,
        };
        let pool_size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt_device,
            pixel_format,
            POOL_BUFFERS,
            pool_size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        // Opcionales según la versión de Windows: si no están, se sigue igual.
        // Sin cursor (Windows 10 2004+): el puntero no tiene que teñir el foco.
        let _ = session.SetIsCursorCaptureEnabled(false);
        // Sin el borde amarillo (Windows 11): hay que pedir el acceso primero.
        let _ = GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
            .and_then(|op| op.join());
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        Ok(Self {
            monitor,
            format,
            pool,
            session,
            pool_size,
            downscaler: None,
        })
    }

    /// Si la sesión sigue sirviendo para el monitor tal como está ahora.
    fn still_valid_for(&self, now: &Monitor) -> bool {
        self.monitor.hmonitor == now.hmonitor
            && self.monitor.advanced_color == now.advanced_color
            && self.monitor.info.width == now.info.width
            && self.monitor.info.height == now.info.height
    }

    /// Toma el cuadro más nuevo, si llegó alguno desde la última vez, y lo reduce.
    fn grab(
        &mut self,
        gpu: &Gpu,
        target: (u32, u32),
        white_scale: f32,
    ) -> ::windows::core::Result<Option<Vec<u8>>> {
        let mut latest = None;
        loop {
            match self.pool.TryGetNextFrame() {
                Ok(frame) => {
                    if let Some(old) = latest.replace(frame) {
                        let _ = old.Close();
                    }
                }
                // Sin cuadro nuevo: la API devuelve null, que windows-rs informa como un
                // error "vacío" (HRESULT S_OK).
                Err(e) if e.code().is_ok() => break,
                Err(e) => return Err(e),
            }
        }
        let Some(frame) = latest else {
            return Ok(None);
        };

        let content = frame.ContentSize()?;
        if content.Width != self.pool_size.Width || content.Height != self.pool_size.Height {
            // Cambió la resolución: se rearma el pool y este cuadro se descarta.
            let _ = frame.Close();
            self.pool_size = content;
            let pixel_format = match self.format {
                PixelFormat::Bgra8 => DirectXPixelFormat::B8G8R8A8UIntNormalized,
                PixelFormat::Rgba16f => DirectXPixelFormat::R16G16B16A16Float,
            };
            self.pool
                .Recreate(&gpu.winrt_device, pixel_format, POOL_BUFFERS, content)?;
            self.downscaler = None;
            return Ok(None);
        }

        let src = (content.Width.max(1) as u32, content.Height.max(1) as u32);
        let reusable = self
            .downscaler
            .as_ref()
            .is_some_and(|d| d.matches(self.format, src, target));
        if !reusable {
            self.downscaler = Some(Downscaler::new(gpu, self.format, src, target)?);
        }
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: la superficie de un cuadro de Graphics Capture es una ID3D11Texture2D.
        let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
        let downscaler = self.downscaler.as_ref().expect("se creó arriba");
        let out = downscaler.process(gpu, &texture, white_scale);
        let _ = frame.Close();
        out.map(Some)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// Pone `None` en el canal, solo si no estaba ya.
fn publish_none(tx: &watch::Sender<Option<Frame>>) {
    tx.send_if_modified(|v| v.take().is_some());
}

fn run(
    request: Request,
    tx: watch::Sender<Option<Frame>>,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), CaptureError>>,
) {
    // SAFETY: se inicializa WinRT para este hilo; se desinicializa al salir.
    let ro = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let backend = |e: ::windows::core::Error| CaptureError::Backend(e.to_string());

    let started = (|| {
        if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
            return Err(CaptureError::Unsupported(
                "Windows Graphics Capture necesita Windows 10 1903 o posterior".into(),
            ));
        }
        let monitor = monitors::find(&request.display)?
            .ok_or_else(|| CaptureError::DisplayNotFound(request.display.clone()))?;
        let gpu = Gpu::new().map_err(backend)?;
        let session = Session::open(&gpu, monitor).map_err(backend)?;
        Ok((gpu, session))
    })();
    let (gpu, first) = match started {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            if ro.is_ok() {
                // SAFETY: par de RoInitialize exitoso en este hilo.
                unsafe { RoUninitialize() };
            }
            return;
        }
    };
    let _ = ready.send(Ok(()));

    // Si no se puede crear la ventana de avisos, se asume pantalla siempre encendida.
    let power = DisplayPower::new().ok();
    let mut session = Some(first);
    let mut next_check = Instant::now() + MONITOR_CHECK;
    let mut next_tick = Instant::now();

    while !stop.load(Ordering::Relaxed) && !tx.is_closed() {
        let now = Instant::now();
        let screen_on = power.as_ref().is_none_or(DisplayPower::is_on);

        if !screen_on {
            session = None;
            publish_none(&tx);
        } else if now >= next_check {
            next_check = now + MONITOR_CHECK;
            match monitors::find(&request.display) {
                Ok(Some(m)) => {
                    let valid = session.as_ref().is_some_and(|s| s.still_valid_for(&m));
                    if !valid {
                        session = None;
                        match Session::open(&gpu, m) {
                            Ok(s) => session = Some(s),
                            Err(_) => {
                                publish_none(&tx);
                                next_check = now + RETRY_AFTER;
                            }
                        }
                    } else if let Some(s) = session.as_mut() {
                        // El blanco SDR se puede cambiar en vivo desde Configuración.
                        s.monitor.sdr_white_scale = m.sdr_white_scale;
                    }
                }
                Ok(None) => {
                    session = None;
                    publish_none(&tx);
                }
                Err(_) => {}
            }
        }

        if let Some(s) = session.as_mut() {
            let white = s.monitor.sdr_white_scale;
            match s.grab(&gpu, request.target, white) {
                Ok(Some(bgra)) => {
                    if let Ok(frame) = Frame::new(request.target.0, request.target.1, bgra) {
                        tx.send_replace(Some(frame));
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    // Se perdió el dispositivo o el monitor: se reintenta en la próxima revisión.
                    session = None;
                    publish_none(&tx);
                    next_check = now + RETRY_AFTER;
                }
            }
        }

        next_tick += request.interval;
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
        } else {
            next_tick = now; // atrasado: no se acumulan tics
        }
    }

    drop(session);
    drop(power);
    publish_none(&tx);
    if ro.is_ok() {
        // SAFETY: par de RoInitialize exitoso en este hilo.
        unsafe { RoUninitialize() };
    }
}
