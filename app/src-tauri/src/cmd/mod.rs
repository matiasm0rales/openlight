//! Las operaciones del backend, repartidas por tema. Cada archivo agrega métodos a
//! [`crate::state::Backend`]; los comandos de `ipc.rs` solo los llaman.

pub mod backup;
pub mod calibration;
pub mod cloud;
pub mod devices;
pub mod diagnostics;
pub mod displays;
pub mod effect;
pub mod legacy;
pub(crate) mod live;
pub mod onboarding;
pub mod session;
pub mod settings;
