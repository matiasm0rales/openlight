//! Actor async por foco: conexión, sesión, latidos, reconexión y DPs → capacidades.
//!
//! Tarea **F (wave 2)** de `docs/PLAN.md`, sección 3.3. Port de `Foco` de `ambilight.py`.

pub mod caps;
pub mod colour;

pub use caps::CapabilitiesExt;
