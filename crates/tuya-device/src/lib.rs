//! Actor async por foco: conexión, sesión, latidos, reconexión y DPs → capacidades.
//!
//! Tarea **F (wave 2)** de `docs/PLAN.md`, sección 3.3. Port de `Foco` de `ambilight.py`.

pub mod caps;
pub mod colour;
pub mod config;
#[allow(dead_code)] // temporal: lo usa el actor (commit siguiente)
mod health;
pub mod original;

pub use caps::CapabilitiesExt;
pub use config::{DeviceConfig, IpMode, Locator, NoLocator, TCP_PORT, Timings};
pub use original::{MemoryStore, SavedState, StateStore, choose_original};
