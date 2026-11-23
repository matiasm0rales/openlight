//! Llavero real del sistema (crate `keyring`).

use openlight_types::Secret;

use crate::{SecretError, SecretKey, SecretStore};

/// Nombre del servicio con el que se registran las entradas en el llavero.
pub const SERVICE: &str = "openlight";

/// Secretos en el llavero del sistema: Keychain en macOS, Credential Manager en Windows y
/// Secret Service (GNOME Keyring, KWallet) en Linux.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringStore;

impl KeyringStore {
    /// Crea el almacén. No toca el llavero hasta el primer uso.
    pub fn new() -> Self {
        Self
    }

    /// Si el llavero del sistema está disponible ahora.
    pub fn check_available() -> Result<(), SecretError> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(()),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }

    fn entry(key: &SecretKey) -> Result<keyring::Entry, SecretError> {
        Self::check_available()?;
        keyring::Entry::new(SERVICE, &key.account()).map_err(|e| backend(key, &e))
    }
}

/// El texto del error del llavero describe la operación, no incluye el valor guardado.
fn backend(key: &SecretKey, error: &keyring::Error) -> SecretError {
    SecretError::Backend {
        entry: key.account(),
        detail: error.to_string(),
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError> {
        Self::entry(key)?
            .set_password(value.expose())
            .map_err(|e| backend(key, &e))
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(backend(key, &e)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(backend(key, &e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toca el llavero real (en macOS puede pedir permiso): se corre a mano con
    /// `cargo test -p secrets -- --ignored`.
    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn cumple_el_contrato_en_el_llavero_real() {
        crate::contract_suite(&KeyringStore::new());
    }
}
