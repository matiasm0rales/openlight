//! # secrets
//!
//! Dónde viven los secretos de OpenLight: la clave local de cada foco y el Access ID / Access
//! Secret de Tuya. **Nunca** van a un JSON ni a un log: se guardan en el llavero del sistema
//! (Keychain, Credential Manager o Secret Service) y siempre circulan envueltos en
//! [`openlight_types::Secret`].
//!
//! - [`SecretStore`]: lo que el resto de la app necesita (guardar, leer, borrar).
//! - [`KeyringStore`]: el llavero real del sistema.
//! - [`MemoryStore`]: un backend en memoria, para pruebas y para correr sin llavero.
//!
//! Los errores no incluyen nunca el valor del secreto, solo el nombre de la entrada.

mod keyring_store;
mod memory;

use std::fmt;

use openlight_types::{DeviceId, ErrorCode, Secret, UserFacingError};

pub use keyring_store::{KeyringStore, SERVICE};
pub use memory::MemoryStore;

/// Qué secreto se guarda.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SecretKey {
    /// Clave local de un foco.
    DeviceLocalKey(DeviceId),
    /// Access ID del proyecto Cloud de Tuya.
    CloudAccessId,
    /// Access Secret del proyecto Cloud de Tuya.
    CloudAccessSecret,
}

impl SecretKey {
    /// Nombre estable de la entrada en el llavero (el "usuario" de la credencial).
    pub fn account(&self) -> String {
        match self {
            Self::DeviceLocalKey(id) => format!("device:{id}:local_key"),
            Self::CloudAccessId => "cloud:access_id".to_owned(),
            Self::CloudAccessSecret => "cloud:access_secret".to_owned(),
        }
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.account())
    }
}

/// Errores del almacén de secretos. Ninguno contiene el valor del secreto.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// No hay llavero disponible (por ejemplo, Linux sin Secret Service o sesión bloqueada).
    #[error("el llavero del sistema no está disponible: {0}")]
    Unavailable(String),
    /// El llavero falló al operar sobre una entrada.
    #[error("error del llavero con la entrada {entry}: {detail}")]
    Backend {
        /// Nombre de la entrada (nunca su valor).
        entry: String,
        /// Descripción del error del llavero.
        detail: String,
    },
}

impl From<SecretError> for UserFacingError {
    fn from(error: SecretError) -> Self {
        UserFacingError::new(ErrorCode::Storage).with_param("detail", error.to_string())
    }
}

/// Almacén de secretos. Las implementaciones son seguras entre hilos.
pub trait SecretStore: Send + Sync {
    /// Guarda (o reemplaza) un secreto.
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError>;
    /// Lee un secreto, o `None` si no existe.
    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError>;
    /// Borra un secreto. Borrar uno que no existe no es un error.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
}

/// Pruebas de contrato que tiene que pasar cualquier [`SecretStore`].
#[cfg(test)]
pub(crate) fn contract_suite(store: &dyn SecretStore) {
    let dev = SecretKey::DeviceLocalKey(DeviceId("bf000000000000000fake1".into()));
    let other = SecretKey::DeviceLocalKey(DeviceId("bf000000000000000fake2".into()));

    assert_eq!(store.get(&dev).unwrap(), None, "empieza vacío");
    store
        .set(&dev, &Secret::new("clave-falsa-0001".into()))
        .unwrap();
    store
        .set(&other, &Secret::new("clave-falsa-0002".into()))
        .unwrap();
    assert_eq!(
        store.get(&dev).unwrap().unwrap().expose(),
        "clave-falsa-0001"
    );

    // Reemplazar
    store
        .set(&dev, &Secret::new("clave-falsa-0003".into()))
        .unwrap();
    assert_eq!(
        store.get(&dev).unwrap().unwrap().expose(),
        "clave-falsa-0003"
    );
    // Las entradas son independientes
    assert_eq!(
        store.get(&other).unwrap().unwrap().expose(),
        "clave-falsa-0002"
    );
    assert_eq!(store.get(&SecretKey::CloudAccessSecret).unwrap(), None);

    // Borrar, y borrar de nuevo
    store.delete(&dev).unwrap();
    store.delete(&dev).unwrap();
    assert_eq!(store.get(&dev).unwrap(), None);
    store.delete(&other).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_nombres_de_entrada_son_estables() {
        assert_eq!(
            SecretKey::DeviceLocalKey(DeviceId("abc".into())).account(),
            "device:abc:local_key"
        );
        assert_eq!(SecretKey::CloudAccessId.account(), "cloud:access_id");
        assert_eq!(
            SecretKey::CloudAccessSecret.account(),
            "cloud:access_secret"
        );
    }

    #[test]
    fn el_error_no_filtra_valores_y_es_visible() {
        let e = SecretError::Backend {
            entry: "cloud:access_secret".into(),
            detail: "denegado".into(),
        };
        assert!(!format!("{e}").contains("clave"));
        let visible: UserFacingError = e.into();
        assert_eq!(visible.code, ErrorCode::Storage);
    }
}
