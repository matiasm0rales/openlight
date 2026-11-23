//! Foco Tuya simulado (TCP 6668 + broadcasts UDP) para pruebas y para desarrollar la UI.
//!
//! Habla 3.1, 3.3, 3.4 y 3.5 como espera tinytuya: consultas, control, latidos, `UPDATEDPS`,
//! negociación de clave de sesión y pushes de `STATUS` cuando cambia un DP. Tiene fallas
//! inyectables en caliente ([`Faults`]) y una API para pruebas ([`SimHandle`]).
//!
//! El framing y la cripto **no** se implementan acá: el simulador depende solo de la interfaz
//! chica de [`wire`] ([`Suite`] y [`Wire`]), que implementa `tuya-proto` (tarea A).

mod config;
mod conn;
mod profile;
mod proto;
mod server;
mod udp;
pub mod wire;

pub use config::{
    BroadcastConfig, DEFAULT_DEV_ID, DEFAULT_LOCAL_KEY, Faults, SimConfig, SimStats, TCP_PORT,
};
pub use profile::{Profile, accepts};
pub use proto::ProtoSuite;
pub use server::{SimHandle, start_with};
pub use udp::default_port;
pub use wire::{Msg, Role, Suite, Wire, WireError, cmd};

/// Arranca un foco simulado con la [`Suite`] real ([`ProtoSuite`]).
///
/// # Errors
/// Si no puede escuchar en `config.bind` (o en los puertos UDP pedidos).
pub async fn start(config: SimConfig) -> std::io::Result<SimHandle> {
    start_with(config, std::sync::Arc::new(ProtoSuite)).await
}
