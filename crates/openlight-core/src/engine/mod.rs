//! La política de envío de un foco: cuándo se manda un color y cuándo no.
//!
//! Es el bucle `ambilight()` de `ambilight.py` convertido en una máquina de estados síncrona:
//! recibe el instante, los cuadros y el estado del enlace, y devuelve las [`Action`]s que hay
//! que ejecutar sobre el foco. No duerme ni hace IO, así que se prueba sin reloj ni foco
//! (la sesión es quien la mueve con tiempo y focos reales).

use color_engine::difference;
use openlight_types::{Hsv, Settings};

use crate::pipeline::{ColorPipeline, Frames, Processed};

/// Qué hacer con el foco después de un paso.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Preparar el foco (`preparar`): guardar el original y ponerlo en modo color.
    Prepare,
    /// Mandar un color.
    Send(Hsv),
    /// Devolver el foco a su estado original (`devolver_estado`).
    Restore,
}

/// Estado del enlace con el foco en este instante (sale de `LinkHealth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    /// Si hay sesión abierta con el foco.
    pub connected: bool,
    /// Reconexiones exitosas hasta ahora.
    pub reconnects: u32,
}

/// Tiempos y umbrales del foco, en segundos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// Entre un paso y el siguiente (`1 / capture_fps`).
    pub frame_period_s: f64,
    /// Mínimo entre dos envíos (`1 / max_sends_per_s`).
    pub send_period_s: f64,
    /// Cambio mínimo, según `difference`, para volver a mandar (`send_threshold`).
    pub send_threshold: f64,
    /// Sin pantalla durante este tiempo, el foco vuelve a su estado (`no_screen_wait_s`).
    pub no_screen_wait_s: f64,
}

impl Timing {
    /// Con los ajustes efectivos del foco.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            frame_period_s: 1.0 / decimal(settings.capture_fps),
            send_period_s: 1.0 / decimal(settings.max_sends_per_s),
            send_threshold: decimal(settings.send_threshold),
            no_screen_wait_s: decimal(settings.no_screen_wait_s),
        }
    }
}

/// `f32` a `f64` por su valor decimal (`0.01`, no `0.009999999776`), como Python.
fn decimal(value: f32) -> f64 {
    value
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(value))
}

/// Lo que dejó un paso.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Step {
    /// Acciones a ejecutar sobre el foco, en orden.
    pub actions: Vec<Action>,
    /// Color de pantalla y color calculado, si hubo pantalla (para `color://live`).
    pub processed: Option<Processed>,
}

/// Máquina de estados de un foco.
#[derive(Debug, Clone)]
pub struct DeviceEngine {
    pipeline: ColorPipeline,
    timing: Timing,
    /// Último color enviado (`enviado`); `None` fuerza el próximo envío.
    sent: Option<Hsv>,
    /// Cuándo se envió el último (`t_envio`).
    last_send_s: Option<f64>,
    /// Reconexiones que se vieron en el paso anterior.
    reconnects: u32,
    /// Desde cuándo no hay pantalla (`t_sin_pantalla`).
    no_screen_since_s: Option<f64>,
    /// Si el foco ya volvió a su estado original (`en_pausa` en el script): por falta de
    /// pantalla, porque el sistema se va a dormir o porque la sesión terminó. Al volver a tener
    /// pantalla hay que prepararlo de nuevo.
    restored: bool,
    /// Si el sistema está suspendido: no se hace nada hasta [`DeviceEngine::resume`].
    suspended: bool,
}

impl DeviceEngine {
    /// Crea la máquina para un foco.
    pub fn new(pipeline: ColorPipeline, timing: Timing) -> Self {
        Self {
            pipeline,
            timing,
            sent: None,
            last_send_s: None,
            reconnects: 0,
            no_screen_since_s: None,
            restored: false,
            suspended: false,
        }
    }

    /// Tiempos del foco.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// El pipeline de color (zonas y suavizado).
    pub fn pipeline(&self) -> &ColorPipeline {
        &self.pipeline
    }

    /// Arranque: hay que preparar el foco antes del primer color.
    pub fn start(&mut self, link: Link) -> Vec<Action> {
        self.reconnects = link.reconnects;
        self.restored = false;
        vec![Action::Prepare]
    }

    /// El último color que se mandó.
    pub fn sent(&self) -> Option<Hsv> {
        self.sent
    }

    /// `Action::Prepare` falló: se vuelve a pedir con el próximo cuadro con pantalla.
    pub fn prepare_failed(&mut self) {
        self.restored = true;
    }

    /// `Action::Restore` falló: el foco sigue sin volver a su estado, así que la restauración
    /// queda pendiente y se reintenta (con el próximo paso sin pantalla o al detener).
    pub fn restore_failed(&mut self) {
        self.restored = false;
    }

    /// `Action::Send` falló: el color sigue pendiente y se reintenta en el próximo envío.
    pub fn send_failed(&mut self) {
        self.sent = None;
    }

    /// Si el foco está devuelto a su estado original (sin pantalla, suspendido o detenido).
    pub fn is_restored(&self) -> bool {
        self.restored
    }

    /// El sistema se va a dormir: hay que dejar el foco como estaba antes de que se corte la red.
    /// Hasta [`DeviceEngine::resume`] los pasos no hacen nada.
    pub fn suspend(&mut self) -> Vec<Action> {
        self.suspended = true;
        self.restore()
    }

    /// El sistema despertó. El foco se vuelve a preparar apenas haya pantalla.
    pub fn resume(&mut self) {
        self.suspended = false;
        self.no_screen_since_s = None;
    }

    /// Fin de la sesión: devuelve el foco a su estado si todavía no lo está.
    pub fn stop(&mut self) -> Vec<Action> {
        self.restore()
    }

    fn restore(&mut self) -> Vec<Action> {
        if self.restored {
            return Vec::new();
        }
        self.restored = true;
        vec![Action::Restore]
    }

    /// Un paso del bucle: `now_s` en segundos de un reloj monótono.
    pub fn tick(&mut self, now_s: f64, frames: &Frames, link: Link) -> Step {
        let mut step = Step::default();
        if self.suspended {
            return step;
        }
        let Some(processed) = self.pipeline.process(now_s, frames) else {
            // Sin pantalla: pasado `no_screen_wait_s`, el foco vuelve a su estado (una sola vez)
            let since = *self.no_screen_since_s.get_or_insert(now_s);
            if now_s - since >= self.timing.no_screen_wait_s {
                step.actions.extend(self.restore());
            }
            return step;
        };
        self.no_screen_since_s = None;
        step.processed = Some(processed);
        let color = processed.color;
        if self.restored {
            // Volvió la pantalla: se retoma desde cero
            step.actions.push(Action::Prepare);
            self.restored = false;
            self.sent = None;
        }

        // Si el foco se reconectó pudo haber perdido su color: se vuelve a mandar
        if link.reconnects != self.reconnects {
            self.reconnects = link.reconnects;
            self.sent = None;
        }
        let due = self
            .last_send_s
            .is_none_or(|t| now_s - t >= self.timing.send_period_s);
        let changed = self
            .sent
            .is_none_or(|sent| difference(color, sent) > self.timing.send_threshold);
        if link.connected && due && changed {
            step.actions.push(Action::Send(color));
            self.sent = Some(color);
            self.last_send_s = Some(now_s);
        }
        step
    }
}

#[cfg(test)]
mod tests;
