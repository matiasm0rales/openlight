//! # openlight-core
//!
//! Runtime de OpenLight (sección 3.8 de `docs/PLAN.md`): pipeline de captura → color → focos,
//! zonas, sesión, pausa y restauración, persistencia, bloqueo por foco e importación del setup
//! del script Python.
//!
//! Lo nativo queda detrás de traits para poder probarlo sin hardware: la pantalla es un
//! [`openlight_types::ScreenSource`] y el foco un `LightDriver`.

pub mod backup;
pub mod driver;
pub mod engine;
pub mod error;
pub mod import;
pub mod locks;
pub mod pipeline;
pub mod power;
pub mod session;
pub mod store;

pub use driver::LightDriver;
pub use engine::{Action, DeviceEngine, Link, Step, Timing};
pub use error::{CoreError, Result};
pub use import::import_legacy;
pub use locks::{DeviceGuard, DeviceLocks, Holder};
pub use pipeline::{ColorPipeline, Frames, Processed};
pub use power::{NativePower, NoNativePower, PowerEvent, PowerEvents};
pub use session::{
    CAPTURE_SIZE, DeviceSpec, DeviceTuning, Session, SessionEvent, SessionOptions, SourceFactory,
};
pub use store::{Config, DeviceRecord, DeviceState, MAX_ZONES, State, Store, next_order};
