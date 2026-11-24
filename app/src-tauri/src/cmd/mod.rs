//! Las operaciones del backend, repartidas por tema. Cada archivo agrega métodos a
//! [`crate::state::Backend`]; los comandos de `ipc.rs` solo los llaman.

pub mod calibration;
pub mod onboarding;
pub mod settings;
