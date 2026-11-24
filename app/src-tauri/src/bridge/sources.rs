//! `SourceFactory` de `openlight-core` sobre el crate `capture`.

use std::sync::Arc;

use openlight_core::SourceFactory;
use openlight_types::{CaptureError, DisplayId, DisplayInfo, Frame, PermissionState, ScreenSource};
use tokio::sync::watch;

/// La fuente de pantalla del sistema operativo (ScreenCaptureKit, Graphics Capture o X11/Wayland).
/// Cada llamada a la fábrica crea una captura nueva: la sesión usa una por monitor.
pub fn platform_sources() -> SourceFactory {
    Arc::new(|| match capture::PlatformCapture::new() {
        Ok(source) => Box::new(source) as Box<dyn ScreenSource>,
        Err(error) => Box::new(BrokenSource(error)),
    })
}

/// Una fuente que no se pudo crear: todo lo que se le pida devuelve el motivo, así el error
/// llega al usuario con su acción en vez de perderse.
#[derive(Debug)]
pub struct BrokenSource(pub CaptureError);

impl ScreenSource for BrokenSource {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Err(self.0.clone())
    }

    fn start(
        &mut self,
        _display: DisplayId,
        _target: (u32, u32),
        _fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError> {
        Err(self.0.clone())
    }

    fn permission(&self) -> PermissionState {
        match self.0 {
            CaptureError::PermissionDenied => PermissionState::Denied,
            _ => PermissionState::NotDetermined,
        }
    }

    fn request_permission(&self) -> PermissionState {
        self.permission()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_fuente_rota_devuelve_su_error_en_todo() {
        let mut source = BrokenSource(CaptureError::Unsupported("sin portal".into()));
        assert!(matches!(
            source.displays(),
            Err(CaptureError::Unsupported(_))
        ));
        assert!(matches!(
            source.start(DisplayId("1".into()), (96, 54), 20.0),
            Err(CaptureError::Unsupported(_))
        ));
        assert_eq!(source.permission(), PermissionState::NotDetermined);
    }

    #[test]
    fn la_fabrica_crea_una_fuente_por_llamada() {
        let factory = platform_sources();
        // No se captura nada: solo que la fábrica arma fuentes independientes
        let _a = factory();
        let _b = factory();
    }
}
