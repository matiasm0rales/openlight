//! Las operaciones del backend, repartidas por tema. Cada archivo agrega métodos a
//! [`crate::state::Backend`]; los comandos de `ipc.rs` solo los llaman.

pub mod calibration;
pub mod cloud;
pub mod devices;
pub mod displays;
pub mod onboarding;
pub mod settings;
