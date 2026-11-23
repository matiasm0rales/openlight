//! El tramo de color del pipeline para un foco: zonas → color de pantalla → objetivo calibrado →
//! suavizado. Es síncrono y no conoce el reloj ni el foco: recibe cuadros y un instante, y devuelve
//! el color. Las decisiones de envío (límites, pausa) están en [`crate::engine`].

use std::collections::HashMap;

use color_engine::{Calibration, ColorParams, Smoother, light_target, screen_color};
use openlight_types::{DisplayId, Frame, Hsv, ScreenColor, Settings, Zone};

/// Último cuadro de cada monitor que se está capturando.
pub type Frames = HashMap<DisplayId, Frame>;

/// Lo que sale de procesar un instante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Processed {
    /// Color de las zonas del foco, tal como se ve en pantalla (para el evento `color://live`).
    pub screen: ScreenColor,
    /// Color ya calibrado y suavizado, listo para mandar.
    pub color: Hsv,
}

/// Pipeline de color de un foco con sus zonas.
#[derive(Debug, Clone)]
pub struct ColorPipeline {
    zones: Vec<Zone>,
    params: ColorParams,
    calibration: Calibration,
    smoother: Smoother,
    /// Último tono de pantalla que no fue `None`, uno por zona (`tono_previo` del script).
    prev_hue: Vec<Option<f32>>,
}

impl ColorPipeline {
    /// Arma el pipeline con los ajustes efectivos del foco y su calibración.
    pub fn new(zones: Vec<Zone>, settings: &Settings, calibration: Calibration) -> Self {
        Self {
            prev_hue: vec![None; zones.len()],
            zones,
            params: ColorParams::from_settings(settings),
            calibration,
            smoother: Smoother::from_settings(settings),
        }
    }

    /// Las zonas que sigue.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// El último color suavizado.
    pub fn smoothed(&self) -> Option<Hsv> {
        self.smoother.color()
    }

    /// Procesa un instante `now_s` (segundos de un reloj monótono).
    ///
    /// Una zona cuyo monitor no tiene cuadro se ignora; si ninguna tiene, no hay pantalla y
    /// devuelve `None`.
    pub fn process(&mut self, now_s: f64, frames: &Frames) -> Option<Processed> {
        let mut colors = Vec::with_capacity(self.zones.len());
        for (i, zone) in self.zones.iter().enumerate() {
            let Some(frame) = frames.get(&zone.display) else {
                continue;
            };
            let color = screen_color(frame, zone.rect, self.prev_hue[i], &self.params);
            if color.h.is_some() {
                self.prev_hue[i] = color.h;
            }
            colors.push(color);
        }
        if colors.is_empty() {
            return None;
        }
        let screen = merge_zones(&colors);
        let target = light_target(
            screen,
            self.smoother.color(),
            &self.calibration,
            &self.params,
        );
        // Después de la calibración: la tabla agranda los cambios de tono en algunas zonas
        let color = self.smoother.update(target, now_s);
        Some(Processed { screen, color })
    }
}

/// Junta los colores de varias zonas en uno solo, que es lo único que un foco puede mostrar.
///
/// Con una zona devuelve esa zona sin tocarla (así un foco con una zona se comporta igual que
/// el script). Con varias: la luz es el promedio, y el tono es la media circular de los tonos
/// de las zonas con color, pesados por `saturación × luz`, así que una zona oscura o desteñida
/// pesa poco. Si los tonos se anulan entre sí (opuestos exactos), gana el de la zona que más pesa.
fn merge_zones(colors: &[ScreenColor]) -> ScreenColor {
    if let [only] = colors {
        return *only;
    }
    let luz = colors.iter().map(|c| c.luz).sum::<f32>() / colors.len() as f32;
    let colored: Vec<(f64, f64, f64)> = colors
        .iter()
        .filter_map(|c| {
            c.h.map(|h| {
                let weight = (f64::from(c.s) * f64::from(c.luz)).max(1e-6);
                (f64::from(h), f64::from(c.s), weight)
            })
        })
        .collect();
    if colored.is_empty() {
        return ScreenColor {
            h: None,
            s: 0.0,
            luz,
        };
    }
    let total: f64 = colored.iter().map(|c| c.2).sum();
    let (x, y) = colored.iter().fold((0.0, 0.0), |(x, y), &(h, _, w)| {
        let angle = h * std::f64::consts::TAU;
        (x + w * angle.cos(), y + w * angle.sin())
    });
    let h = if x.hypot(y) < 1e-9 * total {
        colored
            .iter()
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map_or(0.0, |c| c.0)
    } else {
        (y.atan2(x) / std::f64::consts::TAU).rem_euclid(1.0)
    };
    let s = colored.iter().map(|c| c.1 * c.2).sum::<f64>() / total;
    ScreenColor {
        h: Some(h as f32),
        s: s as f32,
        luz,
    }
}

#[cfg(test)]
mod tests;
