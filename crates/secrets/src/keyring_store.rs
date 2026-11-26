//! Llavero real del sistema (crate `keyring`).

use std::fmt;

use openlight_types::Secret;

use crate::entries::Entries;
use crate::per_key::PerKey;
use crate::vault::Vault;
use crate::{SecretError, SecretKey, SecretStore};

/// Nombre del servicio con el que se registran las entradas en el llavero.
pub const SERVICE: &str = "openlight";

/// Si todos los secretos van juntos en una entrada. macOS pide permiso por cada entrada, así que
/// ahí conviene una sola. En Windows una entrada admite 2560 bytes en UTF-16 (juntos no entrarían
/// ni 20 focos) y nunca se pide permiso; en Linux se desbloquea el llavero entero. En esos dos
/// queda una entrada por secreto.
const SINGLE_ENTRY: bool = cfg!(target_os = "macos");

/// Secretos en el llavero del sistema: Keychain en macOS, Credential Manager en Windows y
/// Secret Service (GNOME Keyring, KWallet) en Linux.
///
/// En macOS todos van en una sola entrada, que se lee una vez y se recuerda en memoria (conviene
/// crear un solo `KeyringStore` por proceso). En Windows y Linux hay una entrada por secreto.
pub struct KeyringStore {
    inner: Box<dyn SecretStore>,
}

impl KeyringStore {
    /// Crea el almacén. No toca el llavero hasta el primer uso.
    pub fn new() -> Self {
        Self {
            inner: layout(OsKeyring, SINGLE_ENTRY),
        }
    }

    /// Si el llavero del sistema está disponible ahora.
    pub fn check_available() -> Result<(), SecretError> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(()),
            Err(e) => Err(SecretError::Unavailable(e.to_string())),
        }
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyringStore").finish_non_exhaustive()
    }
}

/// Cómo se reparten los secretos entre las entradas del llavero.
fn layout<E: Entries + 'static>(entries: E, single_entry: bool) -> Box<dyn SecretStore> {
    if single_entry {
        Box::new(Vault::new(entries))
    } else {
        Box::new(PerKey(entries))
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError> {
        self.inner.set(key, value)
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError> {
        self.inner.get(key)
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.inner.delete(key)
    }
}

/// Las entradas del llavero real.
struct OsKeyring;

impl OsKeyring {
    fn entry(account: &str) -> Result<keyring::Entry, SecretError> {
        KeyringStore::check_available()?;
        keyring::Entry::new(SERVICE, account).map_err(|e| backend(account, &e))
    }
}

/// El texto del error del llavero describe la operación, no incluye el valor guardado.
fn backend(account: &str, error: &keyring::Error) -> SecretError {
    SecretError::Backend {
        entry: account.to_owned(),
        detail: error.to_string(),
    }
}

impl Entries for OsKeyring {
    fn read(&self, account: &str) -> Result<Option<Secret<String>>, SecretError> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(backend(account, &e)),
        }
    }

    fn write(&self, account: &str, value: &Secret<String>) -> Result<(), SecretError> {
        Self::entry(account)?
            .set_password(value.expose())
            .map_err(|e| backend(account, &e))
    }

    fn remove(&self, account: &str) -> Result<(), SecretError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(backend(account, &e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use openlight_types::DeviceId;

    use super::*;
    use crate::entries::fake::FakeEntries;
    use crate::vault::VAULT_ACCOUNT;

    fn accounts_after_saving(single_entry: bool) -> Vec<String> {
        let keychain = FakeEntries::default();
        let store = layout(keychain.clone(), single_entry);
        for key in [
            SecretKey::CloudAccessId,
            SecretKey::DeviceLocalKey(DeviceId("abc".into())),
        ] {
            store.set(&key, &Secret::new("valor-falso".into())).unwrap();
        }
        keychain.accounts()
    }

    #[test]
    fn con_una_sola_entrada_se_juntan_todos() {
        assert_eq!(accounts_after_saving(true), [VAULT_ACCOUNT]);
    }

    #[test]
    fn sin_ella_hay_una_entrada_por_secreto() {
        assert_eq!(
            accounts_after_saving(false),
            ["cloud:access_id", "device:abc:local_key"]
        );
    }

    /// Toca el llavero real (en macOS puede pedir permiso): se corre a mano con
    /// `cargo test -p secrets -- --ignored`.
    #[test]
    #[ignore = "usa el llavero real del sistema"]
    fn cumple_el_contrato_en_el_llavero_real() {
        crate::contract_suite(&KeyringStore::new());
    }
}
