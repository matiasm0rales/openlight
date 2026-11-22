//! # openlight-types
//!
//! Contratos compartidos por todos los crates de OpenLight (sección 2 de `docs/PLAN.md`).
//! Es lo único que las tareas paralelas tienen en común: **cualquier cambio acá pasa por el
//! agente integrador (I-0)** con un PR chico y separado (ver `docs/AGENTS.md`).
//!
//! Convenciones:
//! - Los colores van en `0..1` salvo que el nombre diga otra cosa (`*_deg` = grados).
//! - Los textos que ve el usuario nunca se escriben acá: se usan **claves i18n**
//!   (`settings.saturation.title`, `error.device_key_rejected.detail`, ...) que la UI resuelve
//!   con `app/ui/src/lib/i18n/{es,en}.json`.
//! - Todo lo que cruza el IPC deriva [`specta::Type`] para generar `app/ui/src/lib/bindings.ts`.
//!   Por eso los contadores usan `u32`/`f64` y nunca `u64` (TypeScript no tiene `u64`).

pub mod cloud;
pub mod color;
pub mod device;
pub mod display;
pub mod error;
pub mod ipc;
pub mod profile;
pub mod secret;
pub mod session;
pub mod settings;
pub mod zone;

pub use cloud::*;
pub use color::*;
pub use device::*;
pub use display::*;
pub use error::*;
pub use profile::*;
pub use secret::Secret;
pub use session::*;
pub use settings::*;
pub use zone::*;
