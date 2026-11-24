//! Adaptadores entre los crates: viven acá (y no en `openlight-core`) porque core no puede
//! depender de `tuya-device` ni de `capture`.
//!
//! | adaptador | une |
//! |---|---|
//! | [`driver::TuyaDriver`] | `LightDriver` de core sobre `tuya_device::DeviceHandle` |
//! | [`state_store::CoreStateStore`] | `StateStore` de `tuya-device` sobre el `state.json` del `Store` de core |
//! | [`sources`] | `SourceFactory` de core sobre el crate `capture` |
//! | [`lan::UdpScanner`] | búsqueda de focos en la red sobre `tuya-discovery` |

pub mod driver;
pub mod lan;
pub mod sources;
pub mod state_store;
