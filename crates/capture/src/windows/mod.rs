//! Backend de captura para Windows (tarea E2): Windows Graphics Capture con reducción en la GPU.
//!
//! - [`WindowsCapture`] implementa [`ScreenSource`]. Cada `start` lanza un hilo que captura el
//!   monitor con Graphics Capture (Windows 10 1903+), reduce el cuadro en la GPU con mipmaps
//!   (`GenerateMips`) y publica en el `watch` solo el último cuadro BGRA del tamaño pedido.
//! - HDR: si el monitor tiene "color avanzado" activo, se captura en FP16 (scRGB lineal) y se
//!   pasa a SDR con el nivel de blanco SDR de Windows antes de medir (ver `convert`).
//! - `None` en el canal: el monitor no está (desconectado) o Windows apagó las pantallas.
//! - Ventanas propias: [`exclude_window_from_capture`] las saca de cualquier captura.
//! - Permisos: Windows no pide permiso para capturar la pantalla en apps de escritorio, así que
//!   [`ScreenSource::permission`] devuelve [`PermissionState::NotApplicable`].
//!
//! Las conversiones de `convert` no usan FFI y también se compilan (y prueban) en macOS y Linux.
//
// Se permite `unsafe` solo en este módulo: Graphics Capture, Direct3D 11, DisplayConfig y GDI
// son APIs de Windows que se usan por FFI (crate `windows`), y sus llamadas son `unsafe`. Cada
// bloque lleva su comentario `SAFETY`.
#![allow(unsafe_code)]

mod convert;

#[cfg(target_os = "windows")]
mod gpu;
#[cfg(target_os = "windows")]
mod monitors;
#[cfg(target_os = "windows")]
mod power;
#[cfg(target_os = "windows")]
mod worker;

#[cfg(target_os = "windows")]
pub use imp::{WindowsCapture, exclude_window_from_capture, include_window_in_capture};

#[cfg(target_os = "windows")]
mod imp {
    use std::time::Duration;

    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    use openlight_types::{
        CaptureError, DisplayId, DisplayInfo, Frame, PermissionState, ScreenSource,
    };
    use tokio::sync::watch;

    use super::monitors;
    use super::worker::{Request, Worker};

    /// Fuente de pantalla de Windows. Captura un monitor a la vez; un `start` nuevo reemplaza
    /// al anterior. Para varios monitores, una instancia por monitor.
    #[derive(Default)]
    pub struct WindowsCapture {
        worker: Option<Worker>,
    }

    impl WindowsCapture {
        /// Crea la fuente. No toca la GPU hasta el primer `start`.
        pub fn new() -> Result<Self, CaptureError> {
            Ok(Self::default())
        }
    }

    impl ScreenSource for WindowsCapture {
        fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
            Ok(monitors::enumerate()?.into_iter().map(|m| m.info).collect())
        }

        fn start(
            &mut self,
            display: DisplayId,
            target: (u32, u32),
            fps: f32,
        ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
            self.stop();
            if target.0 == 0 || target.1 == 0 {
                return Err(CaptureError::Backend(format!(
                    "tamaño de cuadro inválido: {}×{}",
                    target.0, target.1
                )));
            }
            let fps = if fps.is_finite() {
                fps.clamp(1.0, 120.0)
            } else {
                20.0
            };
            let (tx, rx) = watch::channel(None);
            let request = Request {
                display,
                target,
                interval: Duration::from_secs_f32(1.0 / fps),
            };
            self.worker = Some(Worker::spawn(request, tx)?);
            Ok(rx)
        }

        fn stop(&mut self) {
            self.worker = None; // `Drop` detiene el hilo y espera a que termine
        }

        fn permission(&self) -> PermissionState {
            PermissionState::NotApplicable
        }

        fn request_permission(&self) -> PermissionState {
            PermissionState::NotApplicable
        }
    }

    /// Saca una ventana propia de toda captura de pantalla (esta, la de otras apps, capturas de
    /// pantalla): en pantalla se sigue viendo, pero en la captura se ve lo que hay detrás.
    /// Sirve para que la ventana de calibración no tiña el color medido.
    ///
    /// `hwnd` es el handle Win32 de una ventana de **este** proceso (por ejemplo, el `hwnd` de
    /// `raw-window-handle` que da Tauri). Necesita Windows 10 2004 o posterior.
    pub fn exclude_window_from_capture(hwnd: isize) -> Result<(), CaptureError> {
        set_affinity(hwnd, true)
    }

    /// Deshace [`exclude_window_from_capture`].
    pub fn include_window_in_capture(hwnd: isize) -> Result<(), CaptureError> {
        set_affinity(hwnd, false)
    }

    fn set_affinity(hwnd: isize, exclude: bool) -> Result<(), CaptureError> {
        let affinity = if exclude {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        // SAFETY: la API valida el handle y falla si no es una ventana de este proceso.
        unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut _), affinity) }
            .map_err(|e| CaptureError::Backend(format!("SetWindowDisplayAffinity falló: {e}")))
    }
}
