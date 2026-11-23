//! Backend de captura para Linux (tarea E3).
//!
//! [`LinuxCapture`] implementa [`ScreenSource`] y elige el backend en tiempo de ejecución
//! ([`LinuxBackend`]):
//!
//! - **X11**: monitores por RandR, cuadro reducido en el servidor con XRender. No pide permisos.
//! - **Wayland**: portal ScreenCast (`ashpd`) con `persist_mode = 2` y PipeWire. El portal
//!   devuelve un `restore_token` que evita el diálogo en las próximas sesiones; `openlight-core`
//!   lo persiste con [`LinuxCapture::restore_token`] / [`LinuxCapture::restore_token_updates`] y
//!   lo devuelve con [`LinuxCapture::set_restore_token`].
//! - **XWayland**: si la sesión es Wayland pero no hay portal ScreenCast. Solo ve ventanas X11.
//!
//! Sin sesión gráfica, o en Wayland sin portal ni XWayland: [`CaptureError::Unsupported`].
//! Para diagnosticar se puede forzar el backend con `OPENLIGHT_CAPTURE=x11` o `=wayland`.
//!
//! No hay código `unsafe` acá: x11rb, wayland-client y ashpd son Rust puro, y el FFI de
//! PipeWire queda encapsulado en el crate `pipewire`.

mod detect;
mod scale;
mod wayland;
mod x11;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use openlight_types::{CaptureError, DisplayId, DisplayInfo, Frame, PermissionState, ScreenSource};
use tokio::sync::watch;

pub use detect::LinuxBackend;

/// Fuente de pantalla de Linux. Ver la documentación del módulo.
pub struct LinuxCapture {
    backend: LinuxBackend,
    x11: x11::Options,
    token: Arc<watch::Sender<Option<String>>>,
    denied: AtomicBool,
    active: Option<Active>,
}

enum Active {
    X11(x11::X11Capture),
    Wayland(wayland::WaylandCapture),
}

impl LinuxCapture {
    /// Detecta la sesión gráfica y elige el backend. En Wayland sondea el portal por D-Bus.
    pub fn new() -> Result<Self, CaptureError> {
        let env = detect::Env::from_process();
        let backend = detect::choose_backend(&env, wayland::probe_portal)?;
        match backend {
            LinuxBackend::XWayland => tracing::warn!(
                "Wayland sin portal ScreenCast: se captura por XWayland y solo se van a ver \
                 las ventanas X11"
            ),
            other => tracing::info!("captura de pantalla con {other:?}"),
        }
        Ok(Self::with_backend(backend))
    }

    /// Usa un backend dado, sin detectar nada.
    pub fn with_backend(backend: LinuxBackend) -> Self {
        Self {
            backend,
            x11: x11::Options::default(),
            token: Arc::new(watch::channel(None).0),
            denied: AtomicBool::new(false),
            active: None,
        }
    }

    /// El backend en uso.
    pub fn backend(&self) -> LinuxBackend {
        self.backend
    }

    /// El último `restore_token` del portal ScreenCast (solo Wayland). Cambia en cada `start`
    /// y en cada `request_permission`: hay que guardar siempre el último, el anterior ya no sirve.
    pub fn restore_token(&self) -> Option<String> {
        self.token.borrow().clone()
    }

    /// Avisa cada vez que el portal entrega un token nuevo, para persistirlo enseguida.
    pub fn restore_token_updates(&self) -> watch::Receiver<Option<String>> {
        self.token.subscribe()
    }

    /// Carga el token guardado de una sesión anterior. Llamala antes de `start`.
    pub fn set_restore_token(&mut self, token: Option<String>) {
        self.token.send_replace(token.filter(|t| !t.is_empty()));
    }
}

impl ScreenSource for LinuxCapture {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        match self.backend {
            LinuxBackend::X11 | LinuxBackend::XWayland => x11::displays(&self.x11),
            LinuxBackend::WaylandPortal => wayland::displays(),
        }
    }

    /// Arranca la captura. En Wayland sin token guardado, el portal muestra el diálogo y esta
    /// llamada espera a que el usuario elija (no la hagas desde un hilo que no pueda bloquearse).
    fn start(
        &mut self,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
        self.stop();
        check_target(target)?;
        let interval = frame_interval(fps)?;
        match self.backend {
            LinuxBackend::X11 | LinuxBackend::XWayland => {
                let (capture, rx) = x11::X11Capture::start(self.x11.clone(), display, target, fps)?;
                self.active = Some(Active::X11(capture));
                Ok(rx)
            }
            LinuxBackend::WaylandPortal => {
                let started = wayland::WaylandCapture::start(
                    display,
                    target,
                    interval,
                    self.x11.recheck,
                    Arc::clone(&self.token),
                );
                match started {
                    Ok((capture, rx)) => {
                        self.denied.store(false, Ordering::Relaxed);
                        self.active = Some(Active::Wayland(capture));
                        Ok(rx)
                    }
                    Err(e) => {
                        if e == CaptureError::PermissionDenied {
                            self.denied.store(true, Ordering::Relaxed);
                        }
                        Err(e)
                    }
                }
            }
        }
    }

    fn stop(&mut self) {
        match self.active.take() {
            Some(Active::X11(mut c)) => c.stop(),
            Some(Active::Wayland(mut c)) => c.stop(),
            None => {}
        }
    }

    fn permission(&self) -> PermissionState {
        match self.backend {
            LinuxBackend::X11 | LinuxBackend::XWayland => PermissionState::NotApplicable,
            LinuxBackend::WaylandPortal if self.denied.load(Ordering::Relaxed) => {
                PermissionState::Denied
            }
            LinuxBackend::WaylandPortal if self.token.borrow().is_some() => {
                PermissionState::Granted
            }
            LinuxBackend::WaylandPortal => PermissionState::NotDetermined,
        }
    }

    /// En Wayland abre el diálogo del portal (el usuario elige el monitor) y guarda el token.
    fn request_permission(&self) -> PermissionState {
        if self.backend != LinuxBackend::WaylandPortal {
            return PermissionState::NotApplicable;
        }
        match wayland::request_permission(&self.token) {
            Ok(()) => {
                self.denied.store(false, Ordering::Relaxed);
                PermissionState::Granted
            }
            Err(CaptureError::PermissionDenied) => {
                self.denied.store(true, Ordering::Relaxed);
                PermissionState::Denied
            }
            Err(e) => {
                tracing::warn!("no se pudo pedir el permiso de captura: {e}");
                PermissionState::NotDetermined
            }
        }
    }
}

impl Drop for LinuxCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn check_target(target: (u32, u32)) -> Result<(), CaptureError> {
    let ok = |side: u32| (1..=4096).contains(&side);
    if ok(target.0) && ok(target.1) {
        Ok(())
    } else {
        Err(CaptureError::Backend(format!(
            "tamaño de cuadro inválido: {}×{}",
            target.0, target.1
        )))
    }
}

/// Intervalo entre cuadros. `fps` se acota a 1–120.
pub(crate) fn frame_interval(fps: f32) -> Result<Duration, CaptureError> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(CaptureError::Backend(format!("fps inválido: {fps}")));
    }
    Ok(Duration::from_secs_f64(
        1.0 / f64::from(fps.clamp(1.0, 120.0)),
    ))
}

/// Publica un cuadro. Un `None` repetido no despierta a nadie.
pub(crate) fn publish(tx: &watch::Sender<Option<Frame>>, frame: Option<Frame>) {
    tx.send_if_modified(|current| {
        if current.is_none() && frame.is_none() {
            return false;
        }
        *current = frame;
        true
    });
}

/// Duerme hasta `deadline` en tramos cortos, para responder rápido a `stop`.
pub(crate) fn sleep_until(deadline: Instant, stop: &AtomicBool) {
    const STEP: Duration = Duration::from_millis(50);
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::sleep((deadline - now).min(STEP));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervalo_entre_cuadros() {
        assert_eq!(frame_interval(20.0).unwrap(), Duration::from_millis(50));
        assert_eq!(
            frame_interval(1000.0).unwrap(),
            Duration::from_secs_f64(1.0 / 120.0)
        );
        assert_eq!(frame_interval(0.5).unwrap(), Duration::from_secs(1));
        assert!(frame_interval(0.0).is_err());
        assert!(frame_interval(f32::NAN).is_err());
    }

    #[test]
    fn tamano_del_cuadro() {
        assert!(check_target((96, 54)).is_ok());
        assert!(check_target((0, 54)).is_err());
        assert!(check_target((96, 5000)).is_err());
    }

    #[test]
    fn publicar_none_dos_veces_no_avisa() {
        let (tx, mut rx) = watch::channel(None);
        publish(&tx, None);
        assert!(!rx.has_changed().unwrap());
        let frame = Frame::new(1, 1, vec![0u8; 4]).unwrap();
        publish(&tx, Some(frame));
        assert!(rx.has_changed().unwrap());
        rx.borrow_and_update();
        publish(&tx, None);
        assert!(rx.has_changed().unwrap());
    }

    #[test]
    fn token_del_portal() {
        let mut capture = LinuxCapture::with_backend(LinuxBackend::WaylandPortal);
        assert_eq!(capture.permission(), PermissionState::NotDetermined);
        let mut updates = capture.restore_token_updates();
        capture.set_restore_token(Some("abc".into()));
        assert!(updates.has_changed().unwrap());
        assert_eq!(updates.borrow_and_update().as_deref(), Some("abc"));
        assert_eq!(capture.restore_token().as_deref(), Some("abc"));
        assert_eq!(capture.permission(), PermissionState::Granted);
        capture.set_restore_token(Some(String::new()));
        assert_eq!(capture.restore_token(), None);
    }

    #[test]
    fn x11_no_tiene_permisos() {
        let capture = LinuxCapture::with_backend(LinuxBackend::X11);
        assert_eq!(capture.permission(), PermissionState::NotApplicable);
        assert_eq!(capture.request_permission(), PermissionState::NotApplicable);
    }

    #[test]
    #[ignore = "necesita GNOME (u otro escritorio) en Wayland con xdg-desktop-portal; abre el \
                diálogo la primera vez"]
    fn captura_por_el_portal_y_reusa_el_token() {
        let mut capture = LinuxCapture::new().unwrap();
        assert_eq!(capture.backend(), LinuxBackend::WaylandPortal);
        let display = capture.displays().unwrap()[0].id.clone();
        let mut rx = capture.start(display.clone(), (96, 54), 20.0).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while rx.borrow_and_update().is_none() {
            assert!(Instant::now() < deadline, "no llegó ningún cuadro");
            std::thread::sleep(Duration::from_millis(50));
        }
        let token = capture.restore_token();
        assert!(token.is_some(), "el portal no devolvió restore_token");
        capture.stop();

        // Segunda sesión con el token: no tiene que aparecer el diálogo.
        let mut again = LinuxCapture::new().unwrap();
        again.set_restore_token(token.clone());
        let _rx = again.start(display, (96, 54), 20.0).unwrap();
        assert_ne!(again.restore_token(), token, "el token es de un solo uso");
    }
}
