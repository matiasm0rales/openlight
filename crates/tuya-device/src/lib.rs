//! Actor async por foco: conexión, sesión, latidos, reconexión y DPs → capacidades.
//!
//! Tarea **F (wave 2)** de `docs/PLAN.md`, sección 3.3. Port de `Foco` de `ambilight.py`.

mod actor;
pub mod caps;
pub mod colour;
pub mod config;
mod conn;
mod handle;
mod health;
pub mod locator;
pub mod original;

pub use caps::CapabilitiesExt;
pub use config::{DeviceConfig, IpMode, Locator, NoLocator, TCP_PORT, Timings};
pub use handle::{DeviceHandle, DeviceSnapshot, StartInfo, spawn};
pub use locator::UdpLocator;
pub use original::{MemoryStore, SavedState, StateStore, choose_original};
