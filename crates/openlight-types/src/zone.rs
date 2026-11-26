//! Zonas: qué parte de qué monitor sigue cada foco.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::DisplayId;

/// Rectángulo normalizado: `x`, `y`, `w`, `h` en `0..1` relativos al monitor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct NormRect {
    /// Borde izquierdo.
    pub x: f32,
    /// Borde superior.
    pub y: f32,
    /// Ancho.
    pub w: f32,
    /// Alto.
    pub h: f32,
}

impl NormRect {
    /// Todo el monitor.
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// Crea un rectángulo.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Si está dentro del monitor y no es vacío.
    pub fn is_valid(&self) -> bool {
        let finite = [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite());
        finite
            && self.x >= 0.0
            && self.y >= 0.0
            && self.w > 0.0
            && self.h > 0.0
            && self.x + self.w <= 1.0 + 1e-6
            && self.y + self.h <= 1.0 + 1e-6
    }

    /// Recorta el rectángulo a un cuadro de `width`×`height` píxeles: `(x0, y0, x1, y1)`
    /// con `x1`/`y1` exclusivos y al menos un píxel de lado.
    pub fn to_pixels(&self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let px = |v: f32, max: u32| ((v.clamp(0.0, 1.0) * max as f32).round() as u32).min(max);
        let x0 = px(self.x, width).min(width.saturating_sub(1));
        let y0 = px(self.y, height).min(height.saturating_sub(1));
        let x1 = px(self.x + self.w, width).max(x0 + 1);
        let y1 = px(self.y + self.h, height).max(y0 + 1);
        (x0, y0, x1, y1)
    }
}

/// Atajos de zona que ofrece la UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ZonePreset {
    /// Todo el monitor.
    Full,
    /// Mitad izquierda.
    Left,
    /// Mitad derecha.
    Right,
    /// Mitad de arriba.
    Top,
    /// Mitad de abajo.
    Bottom,
    /// Cuarto de arriba a la izquierda.
    TopLeft,
    /// Cuarto de arriba a la derecha.
    TopRight,
    /// Cuarto de abajo a la izquierda.
    BottomLeft,
    /// Cuarto de abajo a la derecha.
    BottomRight,
    /// El centro (la mitad del ancho y del alto).
    Center,
    /// Rectángulo dibujado a mano.
    Custom,
}

impl ZonePreset {
    /// Rectángulo del atajo, o `None` para [`ZonePreset::Custom`].
    pub const fn rect(self) -> Option<NormRect> {
        Some(match self {
            Self::Full => NormRect::FULL,
            Self::Left => NormRect::new(0.0, 0.0, 0.5, 1.0),
            Self::Right => NormRect::new(0.5, 0.0, 0.5, 1.0),
            Self::Top => NormRect::new(0.0, 0.0, 1.0, 0.5),
            Self::Bottom => NormRect::new(0.0, 0.5, 1.0, 0.5),
            Self::TopLeft => NormRect::new(0.0, 0.0, 0.5, 0.5),
            Self::TopRight => NormRect::new(0.5, 0.0, 0.5, 0.5),
            Self::BottomLeft => NormRect::new(0.0, 0.5, 0.5, 0.5),
            Self::BottomRight => NormRect::new(0.5, 0.5, 0.5, 0.5),
            Self::Center => NormRect::new(0.25, 0.25, 0.5, 0.5),
            Self::Custom => return None,
        })
    }
}

/// Una región de un monitor que sigue un foco. Un foco puede tener varias zonas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Zone {
    /// Monitor.
    pub display: DisplayId,
    /// Región dentro del monitor.
    pub rect: NormRect,
    /// Atajo del que salió `rect` (o `Custom`).
    pub preset: ZonePreset,
}

impl Zone {
    /// Zona a partir de un atajo. Para `Custom` usa todo el monitor hasta que se dibuje.
    pub fn from_preset(display: DisplayId, preset: ZonePreset) -> Self {
        Self {
            display,
            rect: preset.rect().unwrap_or(NormRect::FULL),
            preset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_atajos_son_validos() {
        for p in [
            ZonePreset::Full,
            ZonePreset::Left,
            ZonePreset::Right,
            ZonePreset::Top,
            ZonePreset::Bottom,
            ZonePreset::Center,
            ZonePreset::TopLeft,
            ZonePreset::TopRight,
            ZonePreset::BottomLeft,
            ZonePreset::BottomRight,
        ] {
            assert!(p.rect().unwrap().is_valid(), "{p:?}");
        }
        assert!(ZonePreset::Custom.rect().is_none());
    }

    #[test]
    fn las_esquinas_son_los_cuatro_cuartos() {
        for (preset, x, y, name) in [
            (ZonePreset::TopLeft, 0.0, 0.0, "top_left"),
            (ZonePreset::TopRight, 0.5, 0.0, "top_right"),
            (ZonePreset::BottomLeft, 0.0, 0.5, "bottom_left"),
            (ZonePreset::BottomRight, 0.5, 0.5, "bottom_right"),
        ] {
            assert_eq!(preset.rect(), Some(NormRect::new(x, y, 0.5, 0.5)), "{name}");
            // Es lo que escriben `devices.json` y la UI
            assert_eq!(serde_json::to_value(preset).unwrap(), name);
        }
    }

    #[test]
    fn rect_a_pixeles() {
        assert_eq!(NormRect::FULL.to_pixels(96, 54), (0, 0, 96, 54));
        assert_eq!(
            ZonePreset::Right.rect().unwrap().to_pixels(96, 54),
            (48, 0, 96, 54)
        );
        // Nunca vacío
        assert_eq!(
            NormRect::new(1.0, 1.0, 0.0, 0.0).to_pixels(96, 54),
            (95, 53, 96, 54)
        );
    }

    #[test]
    fn rect_invalido() {
        assert!(!NormRect::new(0.6, 0.0, 0.5, 1.0).is_valid());
        assert!(!NormRect::new(0.0, 0.0, 0.0, 1.0).is_valid());
        assert!(!NormRect::new(f32::NAN, 0.0, 1.0, 1.0).is_valid());
    }
}
