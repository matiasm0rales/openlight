//! `Calibracion`: traduce un color de pantalla al que hay que pedirle al foco.

use std::collections::BTreeMap;

use openlight_types::ipc::CalibrationPoint;
use serde::Deserialize;

use crate::pymath::{decimal_f64, interp, py_mod};

/// Error al leer una tabla de calibración (`calibracion.json`).
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    /// El archivo no es un JSON con la forma `{"30": {"tono": 26, "sat": 1.3}}`.
    #[error("calibración inválida: {0}")]
    Json(#[from] serde_json::Error),
    /// Una clave no es un número (el tono de pantalla en grados).
    #[error("calibración inválida: la clave {0:?} no es un tono")]
    Key(String),
}

/// Un punto de `calibracion.json`.
#[derive(Debug, Deserialize)]
struct JsonPoint {
    tono: f64,
    #[serde(default = "one")]
    sat: f64,
}

fn one() -> f64 {
    1.0
}

/// Tabla de calibración de un foco (`Calibracion` del script).
///
/// La genera la pantalla de calibración: para cada tono de pantalla, el tono y el factor de
/// saturación que hay que pedirle al foco para que se vea igual. Entre puntos interpola
/// linealmente como `np.interp`, y cierra el círculo repitiendo el primer punto en +360°.
#[doc(alias = "Calibracion")]
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Tonos de pantalla en grados, ordenados, con el primero repetido + 360 al final.
    xs: Vec<f64>,
    /// Tonos del foco en grados (pueden ser negativos o pasar de 360).
    hues: Vec<f64>,
    /// Factores de saturación.
    sats: Vec<f64>,
}

impl Calibration {
    /// Sin calibración: devuelve el mismo color (la tabla `{0: {"tono": 0, "sat": 1.0}}`).
    pub fn identity() -> Self {
        Self::from_table([(0.0, 0.0, 1.0)])
    }

    /// Arma la tabla desde puntos `(tono_pantalla°, tono_foco°, sat)`.
    ///
    /// Como el `dict` de Python, si un tono de pantalla se repite vale el último. Los puntos
    /// con valores no finitos se ignoran. Sin puntos válidos es [`Calibration::identity`].
    pub fn from_table(points: impl IntoIterator<Item = (f64, f64, f64)>) -> Self {
        let mut table: Vec<(f64, f64, f64)> = Vec::new();
        for (x, hue, sat) in points {
            if !(x.is_finite() && hue.is_finite() && sat.is_finite()) {
                continue;
            }
            match table.iter_mut().find(|p| p.0 == x) {
                Some(p) => *p = (x, hue, sat),
                None => table.push((x, hue, sat)),
            }
        }
        if table.is_empty() {
            table.push((0.0, 0.0, 1.0));
        }
        table.sort_by(|a, b| a.0.total_cmp(&b.0));
        let first = table[0];
        let mut xs: Vec<f64> = table.iter().map(|p| p.0).collect();
        let mut hues: Vec<f64> = table.iter().map(|p| p.1).collect();
        let mut sats: Vec<f64> = table.iter().map(|p| p.2).collect();
        // Puntos cerrando el círculo en 360°
        xs.push(first.0 + 360.0);
        hues.push(first.1 + 360.0);
        sats.push(first.2);
        Self { xs, hues, sats }
    }

    /// Desde los puntos del IPC. Los `f32` se toman por su valor decimal (`1.3`, no
    /// `1.2999999523`), como si vinieran de `calibracion.json`.
    pub fn from_points(points: &[CalibrationPoint]) -> Self {
        Self::from_table(points.iter().map(|p| {
            (
                decimal_f64(p.screen_hue_deg),
                decimal_f64(p.tono_deg),
                decimal_f64(p.sat),
            )
        }))
    }

    /// Desde el contenido de `calibracion.json`: `{"30": {"tono": 26, "sat": 1.3}, ...}`.
    /// `sat` es opcional (vale 1).
    pub fn from_json(json: &str) -> Result<Self, CalibrationError> {
        let raw: BTreeMap<String, JsonPoint> = serde_json::from_str(json)?;
        let mut points = Vec::with_capacity(raw.len());
        for (key, p) in raw {
            let x: f64 = key
                .trim()
                .parse()
                .map_err(|_| CalibrationError::Key(key.clone()))?;
            points.push((x, p.tono, p.sat));
        }
        Ok(Self::from_table(points))
    }

    /// Corrige un color (`aplicar`): `h` y `s` en `0..1`, devuelve `(h, s)` en `0..1`.
    /// La saturación nunca pasa de 1.
    #[doc(alias = "aplicar")]
    pub fn apply(&self, h: f64, s: f64) -> (f64, f64) {
        let x = h * 360.0;
        let hue = py_mod(interp(x, &self.xs, &self.hues), 360.0);
        (hue / 360.0, (s * interp(x, &self.xs, &self.sats)).min(1.0))
    }
}

impl Default for Calibration {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_calibracion_json() {
        let cal = Calibration::from_json(r#"{"0": {"tono": 0, "sat": 1.0}, "120": {"tono": 100}}"#)
            .unwrap();
        let (h, s) = cal.apply(120.0 / 360.0, 0.5);
        assert!((h * 360.0 - 100.0).abs() < 1e-9);
        assert_eq!(s, 0.5);
        assert!(matches!(
            Calibration::from_json(r#"{"rojo": {"tono": 0}}"#),
            Err(CalibrationError::Key(_))
        ));
        assert!(Calibration::from_json("[]").is_err());
    }

    #[test]
    fn tabla_vacia_es_identidad() {
        assert_eq!(
            Calibration::from_json("{}").unwrap(),
            Calibration::identity()
        );
        assert_eq!(Calibration::from_points(&[]), Calibration::identity());
    }

    #[test]
    fn desde_puntos_del_ipc() {
        let cal = Calibration::from_points(&[
            CalibrationPoint {
                screen_hue_deg: 0.0,
                tono_deg: 0.0,
                sat: 1.0,
            },
            CalibrationPoint {
                screen_hue_deg: 30.0,
                tono_deg: 26.0,
                sat: 1.3,
            },
        ]);
        let esperada = Calibration::from_table([(0.0, 0.0, 1.0), (30.0, 26.0, 1.3)]);
        assert_eq!(cal, esperada);
    }
}
