//! `objetivo_del_foco` y `diferencia`.

use openlight_types::{Hsv, ScreenColor};

use crate::pymath::py_mod;
use crate::{Calibration, ColorParams};

/// Color a pedirle al foco según lo que hay en pantalla (`objetivo_del_foco`).
///
/// - El brillo crece con la luz de la zona, entre `brightness_min` y `brightness_max`.
/// - Con tono: realza la saturación (`saturation`) y pasa por la calibración.
/// - Sin tono: una escena oscura (luz menor que `dark_screen`) conserva el color `actual`
///   (el del suavizador) y una gris da luz sin color.
#[doc(alias = "objetivo_del_foco")]
pub fn light_target(
    screen: ScreenColor,
    actual: Option<Hsv>,
    calibration: &Calibration,
    params: &ColorParams,
) -> Hsv {
    let luz = f64::from(screen.luz);
    let (lo, hi) = (params.brightness_min, params.brightness_max);
    let v = lo + (hi - lo) * (luz * 1.5).min(1.0);
    if let Some(h) = screen.h {
        let s = (f64::from(screen.s) * params.saturation).min(1.0);
        let (h, s) = calibration.apply(f64::from(h), s);
        return Hsv::new(h as f32, s as f32, v as f32);
    }
    let (h_actual, s_actual) = actual.map_or((0.0, 0.0), |c| (c.h, c.s));
    // Sin tono en pantalla: una escena oscura conserva el color y una gris da luz sin color
    let s = if luz < params.dark_screen {
        s_actual
    } else {
        0.0
    };
    Hsv::new(h_actual, s, v as f32)
}

/// Cuánto se nota el cambio entre dos colores, de 0 a 1 (`diferencia`).
///
/// El tono es circular y solo cuenta tanto como el menos saturado de los dos colores.
#[doc(alias = "diferencia")]
pub fn difference(a: Hsv, b: Hsv) -> f64 {
    difference_f64(
        [f64::from(a.h), f64::from(a.s), f64::from(a.v)],
        [f64::from(b.h), f64::from(b.s), f64::from(b.v)],
    )
}

/// [`difference`] en `f64`, como la usa el suavizador.
pub(crate) fn difference_f64(a: [f64; 3], b: [f64; 3]) -> f64 {
    let hue = (py_mod(a[0] - b[0] + 0.5, 1.0) - 0.5).abs() * 2.0;
    (hue * a[1].min(b[1]))
        .max((a[1] - b[1]).abs())
        .max((a[2] - b[2]).abs())
}
