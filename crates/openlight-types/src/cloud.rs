//! Tuya OpenAPI: regiones, cuenta, dispositivos, mapeos de DPs y el trait [`TuyaCloud`].
//!
//! La nube solo hace falta al configurar y al renovar la clave; el uso diario es 100% local.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{DeviceId, Secret};

/// Data Center de Tuya. Las URLs salen de `tinytuya/Cloud.py::setregion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// Western America (`us`, `az`).
    Us,
    /// Eastern America (`us-e`, `ue`).
    UsEast,
    /// Central Europe (`eu`).
    Eu,
    /// Western Europe (`eu-w`, `we`).
    EuWest,
    /// China (`cn`).
    China,
    /// India (`in`).
    India,
    /// Singapur (`sg`). No estaba en el plan original pero tinytuya lo soporta.
    Singapore,
}

impl Region {
    /// Todas las regiones, en el orden en que se prueban si las credenciales dan "permiso denegado".
    pub const ALL: [Self; 7] = [
        Self::Us,
        Self::Eu,
        Self::UsEast,
        Self::EuWest,
        Self::India,
        Self::Singapore,
        Self::China,
    ];

    /// URL base de la OpenAPI de la región.
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::Us => "https://openapi.tuyaus.com",
            Self::UsEast => "https://openapi-ueaz.tuyaus.com",
            Self::Eu => "https://openapi.tuyaeu.com",
            Self::EuWest => "https://openapi-weaz.tuyaeu.com",
            Self::China => "https://openapi.tuyacn.com",
            Self::India => "https://openapi.tuyain.com",
            Self::Singapore => "https://openapi-sg.iotbing.com",
        }
    }

    /// Código corto que usa tinytuya (`tinytuya.json` → `apiRegion`).
    pub const fn code(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::UsEast => "us-e",
            Self::Eu => "eu",
            Self::EuWest => "eu-w",
            Self::China => "cn",
            Self::India => "in",
            Self::Singapore => "sg",
        }
    }

    /// Lee un código de región de tinytuya, incluidos sus alias.
    pub fn parse(code: &str) -> Option<Self> {
        Some(match code.trim().to_ascii_lowercase().as_str() {
            "us" | "az" => Self::Us,
            "us-e" | "ue" => Self::UsEast,
            "eu" => Self::Eu,
            "eu-w" | "we" => Self::EuWest,
            "cn" => Self::China,
            "in" => Self::India,
            "sg" => Self::Singapore,
            _ => return None,
        })
    }
}

/// Credenciales del proyecto Cloud (paso 6 del asistente). Nunca vuelven a la UI. `Debug` no
/// muestra ni el Access ID ni el Access Secret.
#[derive(Clone, PartialEq, Eq)]
pub struct CloudCredentials {
    /// Access ID / Client ID.
    pub access_id: String,
    /// Access Secret / Client Secret.
    pub access_secret: Secret<String>,
    /// Data Center.
    pub region: Region,
}

impl std::fmt::Debug for CloudCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudCredentials")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

/// Cuenta validada (token obtenido y usuario de Smart Life vinculado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudAccount {
    /// uid del usuario de Smart Life vinculado al proyecto.
    pub uid: String,
    /// Región con la que funcionaron las credenciales.
    pub region: Region,
}

/// Un dispositivo de la cuenta, con su clave local. Uso interno: nunca cruza el IPC
/// (para la UI está [`crate::ipc::CloudDeviceInfo`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudDevice {
    /// Id.
    pub id: DeviceId,
    /// Nombre puesto en Smart Life.
    pub name: String,
    /// Clave local.
    pub local_key: Secret<String>,
    /// Product id (las mismas capacidades para el mismo producto).
    pub product_id: String,
    /// Categoría de Tuya (`dj` = luz, `dd` = tira, `fwd` = ambiente...).
    pub category: String,
    /// Modelo, si la nube lo informa.
    pub model: Option<String>,
    /// Si la nube lo ve conectado.
    pub online: bool,
    /// IP pública con la que se conectó a la nube (no sirve para la red local).
    pub cloud_ip: Option<String>,
}

impl CloudDevice {
    /// Si la categoría es de iluminación.
    pub fn is_light(&self) -> bool {
        is_light_category(&self.category)
    }
}

/// Categorías de Tuya que son luces.
pub fn is_light_category(category: &str) -> bool {
    matches!(
        category,
        "dj" | "dd" | "dc" | "fwd" | "xdd" | "fsd" | "tgq" | "tyndj" | "gyd"
    )
}

/// Esquema de un DP, como lo arma `tinytuya/Cloud.py::_build_mapping`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct DpSchema {
    /// Código (`switch_led`, `colour_data_v2`...).
    pub code: String,
    /// Tipo de Tuya: `Boolean`, `Enum`, `Integer`, `Json`, `String`, `Raw`, `Bitmap`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Valores permitidos (`{"range": [...]}`, `{"min", "max", "scale", "step"}`...).
    #[serde(default)]
    pub values: serde_json::Value,
    /// Texto original para los `Json` (Tuya los ordena y JSON no garantiza orden).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_values: Option<String>,
}

/// Mapeo `dp → esquema` de un producto (el `mapping` de `devices.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct DpMapping(pub BTreeMap<String, DpSchema>);

/// Errores de la nube. `tuya-cloud` (tarea C) los traduce a [`crate::UserFacingError`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CloudError {
    /// Firma inválida: casi siempre el Access Secret está mal copiado.
    #[error("firma inválida")]
    InvalidSignature,
    /// Token inválido o vencido.
    #[error("token inválido")]
    InvalidToken,
    /// Sin permiso: Smart Life no vinculada o región equivocada.
    #[error("permiso denegado")]
    PermissionDenied {
        /// Región con la que sí funcionan las credenciales, si se encontró.
        suggested_region: Option<Region>,
    },
    /// Servicio no autorizado en el proyecto (falta IoT Core o similar).
    #[error("servicio no autorizado")]
    ServiceNotAuthorized,
    /// Venció la prueba gratuita de IoT Core.
    #[error("venció la prueba de IoT Core")]
    TrialExpired,
    /// Otro error de la API, con su código y mensaje originales.
    #[error("error {code} de la API: {msg}")]
    Api {
        /// Código de Tuya.
        code: i64,
        /// Mensaje de Tuya.
        msg: String,
    },
    /// Error de red.
    #[error("error de red: {0}")]
    Network(String),
    /// Respuesta que no se pudo interpretar.
    #[error("respuesta inválida: {0}")]
    Decode(String),
}

/// Cliente de la Tuya OpenAPI. Lo implementa `tuya-cloud` (tarea C).
#[async_trait]
pub trait TuyaCloud: Send + Sync {
    /// Obtiene un token y el uid del usuario vinculado.
    async fn validate(&self) -> Result<CloudAccount, CloudError>;
    /// Lista los dispositivos con su clave local, product id y categoría.
    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError>;
    /// Descarga el mapeo de DPs de un dispositivo.
    async fn mapping(&self, device: &DeviceId) -> Result<DpMapping, CloudError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regiones_ida_y_vuelta() {
        for r in Region::ALL {
            assert_eq!(Region::parse(r.code()), Some(r));
            assert!(r.base_url().starts_with("https://openapi"));
        }
        assert_eq!(Region::parse("AZ"), Some(Region::Us));
        assert_eq!(Region::parse("mars"), None);
    }

    #[test]
    fn el_debug_de_las_credenciales_no_muestra_ni_el_id_ni_el_secreto() {
        let creds = CloudCredentials {
            access_id: "idfalso1234567890abc".into(),
            access_secret: Secret::new("secretofalso0123456789".into()),
            region: Region::Eu,
        };
        let input = crate::ipc::CloudCredentialsInput {
            access_id: creds.access_id.clone(),
            access_secret: creds.access_secret.expose().clone(),
            region: creds.region,
        };
        for text in [format!("{creds:?}"), format!("{input:?}")] {
            assert!(text.contains("Eu"), "{text}");
            assert!(!text.contains("idfalso"), "{text}");
            assert!(!text.contains("secretofalso"), "{text}");
        }
    }

    #[test]
    fn mapeo_de_devices_json() {
        let json = r#"{"20": {"code": "switch_led", "type": "Boolean", "values": {}},
                       "21": {"code": "work_mode", "type": "Enum", "values": {"range": ["white", "colour"]}}}"#;
        let m: DpMapping = serde_json::from_str(json).unwrap();
        assert_eq!(m.0["20"].code, "switch_led");
        assert_eq!(m.0["21"].kind, "Enum");
    }
}
