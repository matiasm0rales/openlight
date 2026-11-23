//! # color-engine
//!
//! Port 1:1, sin cambios de comportamiento, de la matemática de color de `ambilight.py`
//! (sección 3.7 de `docs/PLAN.md`). Son funciones puras: no capturan ni hablan con el foco.
//!
//! | script (`ambilight.py`) | acá |
//! |---|---|
//! | `color_de_pantalla` + `zona_dominante` | [`screen_color`] |
//! | `objetivo_del_foco` | [`light_target`] |
//! | `diferencia` | [`difference`] |
//! | `separacion_tonos` | [`hue_separation`] |
//! | `Calibracion` | [`Calibration`] |
//! | `Suavizador` | [`Smoother`] |
//!
//! Las constantes (`BANDA_NEGRA`, `SIN_COLOR`, `PANTALLA_OSCURA`, `VENTAJA_TONO_PREVIO`, realce,
//! brillos y suavizado) salen del catálogo de ajustes vía [`ColorParams::from_settings`] y
//! [`Smoother::from_settings`].
//!
//! ## Precisión
//!
//! El script calcula las imágenes con numpy en `float32` y el resto en `float64`. Este crate
//! repite esos tipos y el orden de las sumas (ver el módulo `pymath`), y se verifica con las
//! fixtures golden de `fixtures/color`, generadas por el propio script.
//!
//! ## Uso por cuadro
//!
//! ```
//! use color_engine::{Calibration, ColorParams, Smoother, light_target, screen_color};
//! use openlight_types::{Frame, NormRect, Settings};
//!
//! let settings = Settings::default();
//! let params = ColorParams::from_settings(&settings);
//! let calibration = Calibration::identity();
//! let mut smoother = Smoother::from_settings(&settings);
//!
//! // Un cuadro naranja de 96×54 (BGRA)
//! let bgra: Vec<u8> = [20, 130, 240, 255].repeat(96 * 54);
//! let frame = Frame::new(96, 54, bgra).unwrap();
//!
//! // `tono_previo`: el último `h` de pantalla que no fue `None` (uno por zona)
//! let mut prev_hue: Option<f32> = None;
//!
//! let screen = screen_color(&frame, NormRect::FULL, prev_hue, &params);
//! if screen.h.is_some() {
//!     prev_hue = screen.h;
//! }
//! let target = light_target(screen, smoother.color(), &calibration, &params);
//! let to_send = smoother.update(target, 0.0);
//! assert!((to_send.h * 360.0 - 30.0).abs() < 0.5);
//! # let _ = prev_hue;
//! ```

mod calibration;
mod params;
mod pymath;
mod screen;
mod smoother;
mod target;

pub use calibration::{Calibration, CalibrationError};
pub use params::ColorParams;
pub use screen::{HUE_BINS, hue_separation, screen_color};
pub use smoother::Smoother;
pub use target::{difference, light_target};
