//! Constantes del motor de color, tomadas del catálogo de ajustes.

use openlight_types::{ColorMode, Settings};

use crate::pymath::decimal_f64;

/// Ajustes que usa el motor de color, ya convertidos al tipo con el que calcula el script.
///
/// Se arma con [`ColorParams::from_settings`]. Los campos son públicos para las pruebas y para
/// quien quiera probar valores sin pasar por el catálogo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorParams {
    /// `color_mode`: el color dominante o el promedio de la zona (`modo` en el script).
    pub mode: ColorMode,
    /// `black_bar` (`BANDA_NEGRA`): filas y columnas con brillo máximo hasta este valor son
    /// bandas negras y no cuentan para la luz. Se compara en `f32`, como numpy.
    pub black_bar: f32,
    /// `grey_threshold` (`SIN_COLOR`): por debajo de esta saturación la zona no tiene tono.
    pub grey_threshold: f64,
    /// `hue_stickiness` (`VENTAJA_TONO_PREVIO`): cuánto pesa el tono del cuadro anterior.
    pub hue_stickiness: f64,
    /// `dark_screen` (`PANTALLA_OSCURA`): con menos luz que esto se conserva el último color.
    pub dark_screen: f64,
    /// `saturation` (`--saturacion`): realce de la saturación antes de calibrar.
    pub saturation: f64,
    /// `brightness_min` (`--brillo-min`): brillo del foco con la pantalla negra.
    pub brightness_min: f64,
    /// `brightness_max` (`--brillo-max`): brillo del foco con la pantalla a pleno.
    pub brightness_max: f64,
}

impl ColorParams {
    /// Toma los valores efectivos de los ajustes.
    ///
    /// Los ajustes son `f32`; los que el script usa como `float64` se pasan a `f64` por su valor
    /// decimal (`0.02` y no `0.019999999552965164`), así el resultado coincide con Python.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            mode: settings.color_mode,
            black_bar: settings.black_bar,
            grey_threshold: decimal_f64(settings.grey_threshold),
            hue_stickiness: decimal_f64(settings.hue_stickiness),
            dark_screen: decimal_f64(settings.dark_screen),
            saturation: decimal_f64(settings.saturation),
            brightness_min: decimal_f64(settings.brightness_min),
            brightness_max: decimal_f64(settings.brightness_max),
        }
    }

    /// Los mismos parámetros con otro modo de color.
    pub fn with_mode(mut self, mode: ColorMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for ColorParams {
    /// Los defaults del catálogo, que son las constantes de `ambilight.py`.
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_defaults_son_las_constantes_del_script() {
        let p = ColorParams::default();
        assert_eq!(p.mode, ColorMode::Dominant);
        assert_eq!(p.black_bar, 0.06_f32);
        assert_eq!(p.grey_threshold, 0.02);
        assert_eq!(p.dark_screen, 0.12);
        assert_eq!(p.hue_stickiness, 1.4);
        assert_eq!(p.saturation, 1.3);
        assert_eq!(p.brightness_min, 0.05);
        assert_eq!(p.brightness_max, 1.0);
    }
}
