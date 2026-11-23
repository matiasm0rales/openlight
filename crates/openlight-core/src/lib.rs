//! # openlight-core
//!
//! Runtime de OpenLight (sección 3.8 de `docs/PLAN.md`): pipeline de captura → color → focos,
//! zonas, sesión, pausa y restauración, persistencia, bloqueo por foco e importación del setup
//! del script Python.
//!
//! Lo nativo queda detrás de traits para poder probarlo sin hardware: la pantalla es un
//! [`openlight_types::ScreenSource`] y el foco un `LightDriver`.

pub mod error;
pub mod pipeline;
pub mod store;

pub use error::{CoreError, Result};
pub use pipeline::{ColorPipeline, Frames, Processed};
