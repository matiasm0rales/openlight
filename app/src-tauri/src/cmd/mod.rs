//! Las operaciones del backend, repartidas por tema. Cada archivo agrega métodos a
//! [`crate::state::Backend`]; los comandos de `ipc.rs` solo los llaman.

pub mod calibration;
pub mod cloud;
pub mod devices;
pub mod displays;
pub mod effect;
pub mod legacy;
pub mod onboarding;
pub mod session;
pub mod settings;
