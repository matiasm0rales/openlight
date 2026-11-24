//! Lo que el backend le pide a la shell de la app (ventanas y tray) sin conocer a Tauri.
//!
//! En la app real es [`TauriShell`]; en las pruebas no hay shell y el backend funciona igual.

use openlight_types::DisplayId;
use tauri::AppHandle;

/// Los efectos del backend sobre las ventanas y el tray.
pub trait Shell: Send + Sync {
    /// Cambió algo que el tray muestra (sesión, perfiles, focos): hay que redibujar el menú.
    fn changed(&self);
    /// Abre la ventana de calibración a pantalla completa en `display`.
    fn open_calibration(&self, display: &DisplayId) -> Result<(), String>;
    /// Cierra la ventana de calibración (si no está abierta no hace nada).
    fn close_calibration(&self);
}

/// La shell de Tauri.
pub struct TauriShell(pub AppHandle);

impl Shell for TauriShell {
    fn changed(&self) {
        // El menú nativo se arma en el hilo principal
        let app = self.0.clone();
        let _ = self
            .0
            .run_on_main_thread(move || crate::tray::refresh(&app));
    }

    fn open_calibration(&self, display: &DisplayId) -> Result<(), String> {
        crate::windows::open_calibration(&self.0, display.clone()).map_err(|e| e.to_string())
    }

    fn close_calibration(&self) {
        if let Err(error) = crate::windows::close_calibration(&self.0) {
            tracing::warn!(%error, "no se pudo cerrar la ventana de calibración");
        }
    }
}
