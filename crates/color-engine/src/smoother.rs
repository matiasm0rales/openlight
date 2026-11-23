//! `Suavizador`: filtro en el tiempo del color que se manda al foco.

use openlight_types::{Hsv, Settings};

use crate::pymath::{decimal_f64, py_mod};
use crate::target::difference_f64;

/// Filtro en el tiempo del color que se manda al foco (`Suavizador` del script): lento con los
/// cambios chicos, para que no parpadee, y rápido con los cortes de escena.
///
/// La constante de tiempo va de `slow` (cambios chicos) a `slow / 5` (cambios de
/// [`Smoother::BIG_CHANGE`] o más). El tono toma el camino corto del círculo, y desde un color
/// casi blanco salta directo en vez de barrer otros colores. No depende de los cuadros por
/// segundo: cada paso usa el tiempo transcurrido.
#[doc(alias = "Suavizador")]
#[derive(Debug, Clone, PartialEq)]
pub struct Smoother {
    slow: f64,
    fast: f64,
    /// Último color, en `f64` como en Python.
    color: Option<[f64; 3]>,
    /// Momento del último color, en segundos.
    t: f64,
}

impl Smoother {
    /// Desde este cambio (según [`crate::difference`]) se usa la velocidad más rápida
    /// (`CAMBIO_GRANDE`).
    pub const BIG_CHANGE: f64 = 0.25;
    /// Un color con menos saturación que esto no muestra su tono: el tono salta directo.
    const NEAR_WHITE_SAT: f64 = 0.1;

    /// `slow_s`: constante de tiempo en segundos (`lento`). Con 0 o menos no suaviza.
    pub fn new(slow_s: f64) -> Self {
        Self {
            slow: slow_s,
            fast: slow_s / 5.0,
            color: None,
            t: 0.0,
        }
    }

    /// Con el ajuste `smoothing_ms`.
    pub fn from_settings(settings: &Settings) -> Self {
        Self::new(decimal_f64(settings.smoothing_ms) / 1000.0)
    }

    /// El último color, o `None` antes del primer cuadro.
    pub fn color(&self) -> Option<Hsv> {
        self.color
            .map(|[h, s, v]| Hsv::new(h as f32, s as f32, v as f32))
    }

    /// Olvida el color: el próximo cuadro llega directo.
    pub fn reset(&mut self) {
        self.color = None;
    }

    /// Avanza el filtro hacia `target` en el momento `now_s` (segundos de un reloj monótono)
    /// y devuelve el color suavizado (`actualizar`).
    #[doc(alias = "actualizar")]
    pub fn update(&mut self, target: Hsv, now_s: f64) -> Hsv {
        let objetivo = [
            f64::from(target.h),
            f64::from(target.s),
            f64::from(target.v),
        ];
        let Some([h, s, v]) = self.color.filter(|_| self.slow > 0.0) else {
            self.color = Some(objetivo);
            self.t = now_s;
            return target;
        };
        let mix = (difference_f64([h, s, v], objetivo) / Self::BIG_CHANGE).min(1.0);
        let tau = self.slow + (self.fast - self.slow) * mix;
        let step = 1.0 - (-(now_s - self.t) / tau).exp();
        // Un color casi blanco no muestra su tono: salta directo en vez de barrer otros colores
        let hue = if s < Self::NEAR_WHITE_SAT {
            objetivo[0]
        } else {
            py_mod(h + (py_mod(objetivo[0] - h + 0.5, 1.0) - 0.5) * step, 1.0)
        };
        let color = [
            hue,
            s + (objetivo[1] - s) * step,
            v + (objetivo[2] - v) * step,
        ];
        self.color = Some(color);
        self.t = now_s;
        Hsv::new(color[0] as f32, color[1] as f32, color[2] as f32)
    }
}
