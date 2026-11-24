//! Actor async por foco: conexión, sesión, latidos, reconexión y DPs → capacidades.
//!
//! Tarea **F (wave 2)** de `docs/PLAN.md`, sección 3.3. Es el port de la clase `Foco` de
//! `ambilight.py`: cada foco es una tarea de tokio dueña de su socket, que habla con `tuya-proto`
//! (framing y cripto 3.1–3.5) y se maneja desde un [`DeviceHandle`].
//!
//! # Piezas
//!
//! | módulo | qué hace | en `ambilight.py` |
//! |---|---|---|
//! | [`spawn`] / [`DeviceHandle`] | lanza el actor y lo maneja (`start`, `activate`, `send_colour`, `return_to_original`, `restore`…); implementa [`openlight_types::LightDevice`] | `Foco` |
//! | [`caps`] | `DpMapping` → `Capabilities`, con heurística por DPs (1–5 y 20–28) | `DP_*` fijos |
//! | [`colour`] | `V2Hsv12`, `V1Hex14` y `control_data` | `valor_color` |
//! | [`original`] | estado original y [`StateStore`] (lo que persiste `openlight-core`) | `elegir_original`, `leer_estado` |
//! | [`config`] | [`DeviceConfig`], [`Timings`] (con backoff) y el trait [`Locator`] | constantes de `Foco`, `abrir_dispositivo` |
//! | [`locator`] | [`UdpLocator`]: búsqueda de la IP con `tuya-discovery` | `auto_ip` de tinytuya |
//!
//! # Comportamiento
//!
//! - **Coalescencia:** [`DeviceHandle::send_colour`] no espera ni bloquea. Si llegan varios
//!   colores antes de que el actor pueda mandarlos, sale solo el último.
//! - **Conexión:** socket persistente, lectura continua de lo que manda el foco, un latido cada
//!   `heartbeat_s` y la conexión se da por muerta tras `dead_after_s` sin recibir nada.
//!   Reconecta con backoff (`retry_every_s`, el doble, el doble… hasta ×4).
//! - **Al reconectar** con la sesión activa vuelve a poner `switch = true` y `mode = colour`.
//! - **Motivos de corte** (EOF, sin respuesta, timeout, 914, IO) en
//!   [`openlight_types::LinkHealth`], con la hora y la cuenta de la última hora.
//! - **Restauración:** al arrancar se guarda el estado original (y se persiste: si el programa
//!   muere sin restaurar, vale lo guardado); [`DeviceHandle::return_to_original`] pausa y
//!   [`DeviceHandle::restore`] sale.
//! - **IP y versión:** la guardada primero y, si falla o da 914, discovery UDP. Con IP fija
//!   ([`IpMode::Fixed`]) nunca se busca otra. Si el foco anuncia otra versión del protocolo
//!   que la configurada (un registro 3.3 para un foco 3.5), el actor usa la anunciada, la
//!   guarda con la IP ([`SavedState::version`]) y la informa en [`StartInfo::version`].
//!
//! # Ejemplo
//!
//! ```no_run
//! use std::sync::Arc;
//! use openlight_types::{DeviceCreds, DeviceId, Hsv, ProtocolVersion, Secret};
//! use tuya_device::{DeviceConfig, MemoryStore, UdpLocator, spawn};
//!
//! # async fn demo() -> Result<(), openlight_types::DeviceError> {
//! let creds = DeviceCreds {
//!     id: DeviceId("id-del-foco".into()),
//!     local_key: Secret::new("0123456789abcdef".into()),
//!     version: ProtocolVersion::V35,
//!     ip: None,
//! };
//! let device = spawn(
//!     DeviceConfig::new(creds),
//!     Arc::new(UdpLocator::new()),
//!     Arc::new(MemoryStore::new()), // en la app, el StateStore de openlight-core
//! );
//! let info = device.start().await?; // espera al foco y guarda su estado original
//! device.activate().await?; // switch = true, mode = colour
//! device.send_colour(Hsv::new(0.33, 1.0, 1.0)); // no espera: coalesce
//! device.restore().await?; // devuelve el foco a como estaba y cierra
//! # let _ = info;
//! # Ok(())
//! # }
//! ```

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
pub use config::{DeviceConfig, IpMode, Located, Locator, NoLocator, TCP_PORT, Timings};
pub use handle::{DeviceHandle, DeviceSnapshot, StartInfo, spawn};
pub use locator::UdpLocator;
pub use original::{MemoryStore, SavedState, StateStore, choose_original};
