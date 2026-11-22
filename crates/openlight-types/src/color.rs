//! Colores que circulan por el pipeline.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Color HSV con los tres componentes en `0..1` (`h` = 1 equivale a 360°).
///
/// Es lo que se le pide al foco, después de calibración y suavizado.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Hsv {
    /// Tono, `0..1`.
    pub h: f32,
    /// Saturación, `0..1`.
    pub s: f32,
    /// Brillo, `0..1`.
    pub v: f32,
}

impl Hsv {
    /// Crea un color HSV.
    pub const fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }
}

/// Color representativo de una zona de la pantalla (salida de `color_de_pantalla`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct ScreenColor {
    /// Tono en `0..1`, o `None` si la zona no tiene color (negro o gris).
    pub h: Option<f32>,
    /// Saturación en `0..1` (0 cuando `h` es `None`).
    pub s: f32,
    /// Luz media de la zona en `0..1`, sin contar las bandas negras.
    pub luz: f32,
}

/// Cómo se resume una zona en un solo color (ajuste `color_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    /// El color que más se ve (histograma de tonos). Es el modo `dominante` del script.
    Dominant,
    /// La mezcla de toda la zona. Es el modo `promedio` del script.
    Average,
}

impl ColorMode {
    /// Identificador estable (el mismo que usa serde).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dominant => "dominant",
            Self::Average => "average",
        }
    }

    /// Lee el nombre estable o el nombre del script Python (`dominante` / `promedio`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "dominant" | "dominante" => Some(Self::Dominant),
            "average" | "promedio" => Some(Self::Average),
            _ => None,
        }
    }
}
