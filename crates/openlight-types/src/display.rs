//! Pantallas y el trait [`ScreenSource`] que implementan los backends de captura.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::watch;

/// Identificador de monitor, estable mientras el monitor siga conectado.
///
/// macOS: `CGDirectDisplayID`; Windows: nombre del dispositivo (`\\.\DISPLAY1`);
/// Linux: nombre de la salida X11/Wayland o el nodo de PipeWire.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct DisplayId(pub String);

impl fmt::Display for DisplayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un monitor disponible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DisplayInfo {
    /// Identificador.
    pub id: DisplayId,
    /// Nombre para mostrar.
    pub name: String,
    /// Ancho en píxeles reales.
    pub width: u32,
    /// Alto en píxeles reales.
    pub height: u32,
    /// Si es la pantalla integrada (la de la notebook).
    pub builtin: bool,
    /// Si es el monitor principal.
    pub primary: bool,
}

/// Un cuadro reducido (~96×54) en BGRA, 4 bytes por píxel, filas sin relleno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Ancho en píxeles.
    pub width: u32,
    /// Alto en píxeles.
    pub height: u32,
    /// Píxeles BGRA (`width * height * 4` bytes).
    pub bgra: Arc<[u8]>,
}

impl Frame {
    /// Crea un cuadro y verifica el tamaño del búfer.
    pub fn new(width: u32, height: u32, bgra: impl Into<Arc<[u8]>>) -> Result<Self, CaptureError> {
        let bgra = bgra.into();
        let expected = width as usize * height as usize * 4;
        if bgra.len() != expected {
            return Err(CaptureError::Backend(format!(
                "búfer de {} bytes, se esperaban {expected}",
                bgra.len()
            )));
        }
        Ok(Self {
            width,
            height,
            bgra,
        })
    }
}

/// Estado de un permiso del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// Concedido.
    Granted,
    /// Denegado: hay que ir a los ajustes del sistema.
    Denied,
    /// Todavía no se pidió.
    NotDetermined,
    /// No existe en este sistema (por ejemplo, Grabación de pantalla en Windows).
    NotApplicable,
}

/// Errores de captura.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// Falta el permiso de captura.
    #[error("falta el permiso de captura de pantalla")]
    PermissionDenied,
    /// El sistema dice que hay permiso, pero niega la captura (macOS).
    #[error("el permiso de captura figura concedido, pero el sistema niega la captura")]
    PermissionStale,
    /// El monitor pedido no está (desconectado o dormido).
    #[error("el monitor {0} no está disponible")]
    DisplayNotFound(DisplayId),
    /// El sistema no soporta captura (por ejemplo, Wayland sin portal ScreenCast).
    #[error("captura no soportada: {0}")]
    Unsupported(String),
    /// Error del backend.
    #[error("error del backend de captura: {0}")]
    Backend(String),
}

/// Fuente de cuadros de pantalla. Una implementación por sistema (`capture`, tareas E1–E3).
///
/// La captura corre en un hilo propio y publica solo el último cuadro en un canal `watch`;
/// `None` significa "ahora no hay pantalla" (monitor dormido o desconectado).
pub trait ScreenSource: Send {
    /// Monitores disponibles ahora.
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    /// Empieza a capturar `display` reducido a `target` (ancho, alto) a `fps` cuadros por segundo.
    fn start(
        &mut self,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> Result<watch::Receiver<Option<Frame>>, CaptureError>;
    /// Deja de capturar. Llamarla sin captura activa no hace nada.
    fn stop(&mut self) {}
    /// Estado del permiso de captura, sin pedirlo.
    fn permission(&self) -> PermissionState;
    /// Pide el permiso (puede abrir un diálogo del sistema) y devuelve el estado resultante.
    fn request_permission(&self) -> PermissionState;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_verifica_el_tamano() {
        assert!(Frame::new(2, 1, vec![0u8; 8]).is_ok());
        assert!(Frame::new(2, 1, vec![0u8; 7]).is_err());
    }
}
