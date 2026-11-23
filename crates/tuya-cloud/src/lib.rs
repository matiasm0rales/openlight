//! Cliente de la Tuya OpenAPI: firma, token, dispositivos, claves locales, mapeos de DPs y regiones.
//!
//! Implementa [`openlight_types::TuyaCloud`] (sección 3.4 de `docs/PLAN.md`). Todo está verificado
//! contra `tinytuya/Cloud.py` y `wizard.py` (tinytuya 1.20):
//!
//! - **Firma** ([`sign`]): `_tuyaplatform` con `new_sign_algorithm=True`.
//! - **Token**: `GET /v1.0/token?grant_type=1`; se renueva solo cuando está por vencer
//!   (`expire_time`) o cuando Tuya responde "token invalid" (se reintenta una vez, como tinytuya).
//!   A diferencia de tinytuya, **no** se manda el Access Secret en un encabezado `secret` al pedir
//!   el token: la firma alcanza y la documentación de Tuya no lo pide.
//! - **Dispositivos**: `getdevices` sin `apiDeviceID`, o sea `_get_all_devices` sobre
//!   `/v1.0/iot-01/associated-users/devices` con paginado `last_row_key`, y una segunda pasada por
//!   uid en `/v1.3/iot-03/devices` que completa las claves locales que falten
//!   (`_update_device_list`).
//! - **Mapeo**: `getmapping` / `_build_mapping` con `GET /v1.1/devices/{id}/specifications`,
//!   cacheado por product id.
//! - **Regiones**: si da "permiso denegado" (1106) se prueban las otras regiones de
//!   [`openlight_types::Region::ALL`] y se sugiere la que funcione.
//! - **Errores** ([`error`]): tabla de códigos con su fuente y traducción a `UserFacingError`.
//!
//! La nube solo hace falta al configurar y al renovar la clave: el uso diario es 100% local.

pub mod client;
pub mod error;
pub mod mapping;
pub mod sign;

pub use client::TuyaCloudClient;
pub use error::to_user_facing;
