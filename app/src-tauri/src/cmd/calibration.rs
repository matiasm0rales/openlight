//! Calibración de un foco (reemplaza a `calibrar.py`): la tabla "tono de pantalla → tono y
//! saturación que hay que pedirle al foco para que se vea igual".
//!
//! Esto es la lógica: mover el foco en vivo y guardar la tabla. La ventana a pantalla completa
//! en el monitor elegido la abre la shell (tarea I1b).

use color_engine::Calibration;
use openlight_core::{DeviceGuard, Holder};
use openlight_types::ipc::{CalibrationPoint, CalibrationState};
use openlight_types::{DeviceId, DisplayId, Hsv};
use tuya_device::DeviceHandle;

use crate::errors::{CmdResult, device_error, invalid};
use crate::state::Backend;

/// Tonos de pantalla del modo completo (`TONOS` de `calibrar.py`).
const FULL_HUES: [f32; 12] = [
    0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0, 330.0,
];
/// Tonos del modo rápido.
const QUICK_HUES: [f32; 6] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0];
/// El tono del foco se mueve como mucho esto alrededor del de pantalla (`min`/`max` del slider).
const HUE_RANGE_DEG: f32 = 90.0;
/// Rango del factor de saturación.
const SAT_RANGE: (f32, f32) = (0.3, 1.5);

/// Una calibración en curso: el foco conectado, su candado y la tabla que se va armando.
pub(crate) struct CalibrationRun {
    device: DeviceId,
    display: DisplayId,
    points: Vec<CalibrationPoint>,
    index: u32,
    handle: DeviceHandle,
    /// Mientras viva, ni la sesión ni el control manual pueden tomar el foco.
    _guard: DeviceGuard,
}

impl CalibrationRun {
    fn state(&self) -> CalibrationState {
        CalibrationState {
            device: self.device.clone(),
            display: self.display.clone(),
            points: self.points.clone(),
            index: self.index,
        }
    }

    /// Mueve el foco al color del punto `index` (`/foco` de `calibrar.py`).
    fn show(&self, index: usize) {
        if let Some(point) = self.points.get(index) {
            self.handle.send_colour(point_colour(point));
        }
    }
}

/// El color que se le pide al foco: tono y saturación del punto, al brillo máximo.
fn point_colour(point: &CalibrationPoint) -> Hsv {
    Hsv::new(
        point.tono_deg.rem_euclid(360.0) / 360.0,
        point.sat.min(1.0),
        1.0,
    )
}

/// Los puntos de partida: lo guardado y, donde no hay, lo que da la tabla guardada
/// (`tabla_inicial`), con el tono más cercano al de pantalla.
fn starting_points(hues: &[f32], saved: &[CalibrationPoint]) -> Vec<CalibrationPoint> {
    let calibration = Calibration::from_points(saved);
    hues.iter()
        .map(|&hue| {
            if let Some(point) = saved.iter().find(|p| p.screen_hue_deg == hue) {
                return *point;
            }
            let (h, _) = calibration.apply(f64::from(hue) / 360.0, 1.0);
            let tono = (h * 360.0).round() as f32;
            CalibrationPoint {
                screen_hue_deg: hue,
                tono_deg: if (tono - hue).abs() <= 180.0 {
                    tono
                } else {
                    tono - 360.0
                },
                sat: 1.0,
            }
        })
        .collect()
}

impl Backend {
    /// Abre la calibración de un foco: lo conecta, lo pone en modo color y arranca en el primer
    /// tono. `quick` usa 6 tonos en vez de 12.
    pub async fn calibration_start(
        &self,
        device: DeviceId,
        display: DisplayId,
        quick: bool,
    ) -> CmdResult<CalibrationState> {
        let mut slot = self.calibration.lock().await;
        if let Some(previous) = slot.take() {
            self.close_calibration(previous).await;
        }
        let record = self.record(&device)?;
        let guard = self.locks.try_acquire(&device, Holder::Calibration)?;
        self.release_manual(&device);

        let saved = self.store.load_calibration(&device)?.unwrap_or_default();
        let hues: &[f32] = if quick { &QUICK_HUES } else { &FULL_HUES };
        let points = starting_points(hues, &saved);

        let handle = self.spawn_device(&record)?;
        let connect = tokio::time::timeout(self.options.connect_timeout, handle.start()).await;
        match connect {
            Ok(result) => {
                result.map_err(|e| device_error(&device, &e))?;
            }
            Err(_) => {
                return Err(device_error(
                    &device,
                    &openlight_types::DeviceError::NotFound,
                ));
            }
        }
        handle
            .activate()
            .await
            .map_err(|e| device_error(&device, &e))?;

        let run = CalibrationRun {
            device,
            display,
            points,
            index: 0,
            handle,
            _guard: guard,
        };
        // La ventana a pantalla completa en el monitor elegido (la shell de la app); sin ella la
        // calibración no sirve, así que si no abre se deja todo como estaba.
        if let Some(shell) = self.shell()
            && let Err(detail) = shell.open_calibration(&run.display)
        {
            self.close_calibration(run).await;
            return Err(crate::errors::internal(detail));
        }
        run.show(0);
        let state = run.state();
        *slot = Some(run);
        Ok(state)
    }

    /// Ajusta el punto `index` (`None` = no cambiar ese valor) y mueve el foco en vivo.
    /// El tono se acota a ±90° del de pantalla y la saturación a 0.3–1.5.
    pub async fn calibration_set(
        &self,
        index: u32,
        tono_deg: Option<f32>,
        sat: Option<f32>,
    ) -> CmdResult<CalibrationState> {
        let mut slot = self.calibration.lock().await;
        let run = slot
            .as_mut()
            .ok_or_else(|| invalid("calibration", "no hay una calibración en curso"))?;
        let point = run
            .points
            .get_mut(index as usize)
            .ok_or_else(|| invalid("calibration", format!("no existe el tono {index}")))?;
        if let Some(tono) = tono_deg.filter(|t| t.is_finite()) {
            let base = point.screen_hue_deg;
            point.tono_deg = tono.clamp(base - HUE_RANGE_DEG, base + HUE_RANGE_DEG);
        }
        if let Some(sat) = sat.filter(|s| s.is_finite()) {
            point.sat = (sat.clamp(SAT_RANGE.0, SAT_RANGE.1) * 100.0).round() / 100.0;
        }
        run.index = index;
        run.show(index as usize);
        Ok(run.state())
    }

    /// Guarda la tabla del foco y cierra la calibración. En modo rápido se conservan los demás
    /// tonos que ya estaban guardados.
    pub async fn calibration_save(&self) -> CmdResult<()> {
        let mut slot = self.calibration.lock().await;
        let run = slot
            .take()
            .ok_or_else(|| invalid("calibration", "no hay una calibración en curso"))?;
        let saved = self
            .store
            .load_calibration(&run.device)?
            .unwrap_or_default();
        let mut table: Vec<CalibrationPoint> = saved
            .into_iter()
            .filter(|old| {
                !run.points
                    .iter()
                    .any(|new| new.screen_hue_deg == old.screen_hue_deg)
            })
            .collect();
        // Como `guardar()` del script: tono entero y saturación con dos decimales
        table.extend(run.points.iter().map(|p| CalibrationPoint {
            tono_deg: p.tono_deg.round(),
            sat: (p.sat * 100.0).round() / 100.0,
            ..*p
        }));
        table.sort_by(|a, b| a.screen_hue_deg.total_cmp(&b.screen_hue_deg));
        let saved = self.store.save_calibration(&run.device, &table);
        // Pase lo que pase con el disco, el foco vuelve a su estado
        self.close_calibration(run).await;
        Ok(saved?)
    }

    /// Cierra la calibración sin guardar y devuelve el foco a su estado.
    pub async fn calibration_cancel(&self) -> CmdResult<()> {
        if let Some(run) = self.calibration.lock().await.take() {
            self.close_calibration(run).await;
        }
        Ok(())
    }

    /// Devuelve el foco a su estado original y suelta el candado.
    async fn close_calibration(&self, run: CalibrationRun) {
        if let Some(shell) = self.shell() {
            shell.close_calibration();
        }
        let CalibrationRun {
            device,
            handle,
            _guard,
            ..
        } = run;
        let wait = self.options.restore_timeout;
        match tokio::time::timeout(wait, handle.restore()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%device, %error, "el foco no volvió a su estado tras calibrar");
            }
            Err(_) => tracing::warn!(%device, "el foco no volvió a su estado a tiempo"),
        }
        // Libre de nuevo: si participa del efecto en marcha, vuelve a entrar
        drop(_guard);
        self.config_changed();
    }
}

impl Backend {
    /// Si hay una calibración en curso (para que la shell sepa si cerrar su ventana).
    pub async fn calibrating(&self) -> Option<DeviceId> {
        self.calibration
            .lock()
            .await
            .as_ref()
            .map(|r| r.device.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(hue: f32, tono: f32, sat: f32) -> CalibrationPoint {
        CalibrationPoint {
            screen_hue_deg: hue,
            tono_deg: tono,
            sat,
        }
    }

    #[test]
    fn sin_calibracion_cada_tono_parte_sin_correccion() {
        // port: calibrar.py::tabla_inicial
        let points = starting_points(&FULL_HUES, &[]);
        assert_eq!(points.len(), 12);
        for point in &points {
            assert_eq!(point.tono_deg, point.screen_hue_deg);
            assert_eq!(point.sat, 1.0);
        }
    }

    #[test]
    fn lo_guardado_manda_y_lo_que_falta_sale_de_interpolar() {
        // port: calibrar.py::tabla_inicial
        let table = [saved(0.0, 10.0, 1.2), saved(120.0, 100.0, 0.9)];
        let points = starting_points(&QUICK_HUES, &table);
        assert_eq!(points[0], table[0]);
        assert_eq!(points[2], table[1]);
        // El tono de 60° queda entre los dos (interpolación circular)
        assert!(
            (points[1].tono_deg - 55.0).abs() <= 1.0,
            "{}",
            points[1].tono_deg
        );
        assert_eq!(
            points[1].sat, 1.0,
            "la saturación de partida es 1, como el script"
        );
    }

    #[test]
    fn el_color_del_foco_gira_alrededor_de_cero_y_acota_la_saturacion() {
        let c = point_colour(&saved(0.0, -30.0, 1.5));
        assert!((c.h - 330.0 / 360.0).abs() < 1e-6);
        assert_eq!(c.s, 1.0);
        assert_eq!(c.v, 1.0);
        let c = point_colour(&saved(330.0, 380.0, 0.5));
        assert!((c.h - 20.0 / 360.0).abs() < 1e-6);
        assert_eq!(c.s, 0.5);
    }
}
