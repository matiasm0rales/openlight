//! Todos los secretos en una sola entrada del llavero.
//!
//! macOS pide permiso por cada entrada, así que con una por secreto había que poner la contraseña
//! varias veces en cada versión nueva. [`Vault`] los junta en un JSON dentro de una única entrada,
//! la lee una sola vez y la recuerda en memoria.
//!
//! Las versiones anteriores guardaban una entrada por secreto, con el nombre de
//! [`SecretKey::account`]. La primera vez que se pide una clave que la entrada nueva no conoce, se
//! busca la anterior, se la pasa a la nueva y se la borra. El resultado queda anotado (aunque no
//! hubiera nada), así no se vuelve a buscar en cada arranque.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use openlight_types::Secret;
use serde::{Deserialize, Serialize};

use crate::entries::Entries;
use crate::{SecretError, SecretKey, SecretStore};

/// Nombre de la entrada que junta todos los secretos.
pub(crate) const VAULT_ACCOUNT: &str = "secrets";

/// Versión del formato guardado. Una versión desconocida no se lee ni se pisa.
const VERSION: u32 = 1;

/// Lo que hay en la entrada.
#[derive(Clone, Deserialize)]
struct Contents {
    version: u32,
    /// Por nombre de cuenta. `None` anota que ese secreto no existe (se borró, o no había una
    /// entrada anterior), así no se la busca de nuevo.
    secrets: BTreeMap<String, Option<Secret<String>>>,
}

/// [`Contents`] para escribirlo: `Secret` no se serializa a propósito, así que el valor se expone
/// acá, y solo para armar el texto que vuelve a quedar envuelto en `Secret`.
#[derive(Serialize)]
struct Written<'a> {
    version: u32,
    secrets: BTreeMap<&'a str, Option<&'a str>>,
}

impl Contents {
    fn empty() -> Self {
        Self {
            version: VERSION,
            secrets: BTreeMap::new(),
        }
    }

    /// Los errores no llevan el de `serde_json`, que puede citar el contenido.
    fn decode(text: &Secret<String>) -> Result<Self, SecretError> {
        match serde_json::from_str::<Self>(text.expose()) {
            Ok(contents) if contents.version == VERSION => Ok(contents),
            _ => Err(vault_error(
                "el contenido guardado no se puede interpretar (¿lo escribió una versión más \
                 nueva?)",
            )),
        }
    }

    fn encode(&self) -> Result<Secret<String>, SecretError> {
        let written = Written {
            version: self.version,
            secrets: self
                .secrets
                .iter()
                .map(|(account, value)| {
                    let value = value.as_ref().map(|secret| secret.expose().as_str());
                    (account.as_str(), value)
                })
                .collect(),
        };
        serde_json::to_string(&written)
            .map(Secret::new)
            .map_err(|_| vault_error("no se pudo armar el contenido a guardar"))
    }
}

fn vault_error(detail: &str) -> SecretError {
    SecretError::Backend {
        entry: VAULT_ACCOUNT.to_owned(),
        detail: detail.to_owned(),
    }
}

/// Todos los secretos en una sola entrada, con copia en memoria.
pub(crate) struct Vault<E> {
    entries: E,
    /// `None` hasta la primera lectura que salga bien. El candado se mantiene mientras se habla
    /// con el llavero: así no aparecen varios pedidos de contraseña a la vez ni se pisan dos
    /// escrituras.
    cache: Mutex<Option<Contents>>,
}

impl<E: Entries> Vault<E> {
    pub(crate) fn new(entries: E) -> Self {
        Self {
            entries,
            cache: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Contents>> {
        // La copia en memoria solo se reemplaza entera: un pánico no la deja a medias.
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// El contenido, leído del llavero solo la primera vez. Un error no se recuerda (la próxima
    /// vez se reintenta) y corta la operación, para no pisar lo que no se pudo leer.
    fn load<'a>(&self, cache: &'a mut Option<Contents>) -> Result<&'a Contents, SecretError> {
        let contents = match cache.take() {
            Some(contents) => contents,
            None => match self.entries.read(VAULT_ACCOUNT)? {
                Some(text) => Contents::decode(&text)?,
                None => Contents::empty(),
            },
        };
        Ok(cache.insert(contents))
    }

    /// Anota un secreto en el llavero y, solo si se pudo escribir, en la copia en memoria.
    fn store(
        &self,
        cache: &mut Option<Contents>,
        account: String,
        value: Option<Secret<String>>,
    ) -> Result<(), SecretError> {
        let mut next = self.load(cache)?.clone();
        next.secrets.insert(account, value);
        self.entries.write(VAULT_ACCOUNT, &next.encode()?)?;
        *cache = Some(next);
        Ok(())
    }
}

impl<E: Entries> SecretStore for Vault<E> {
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError> {
        let account = key.account();
        let mut cache = self.lock();
        let unknown = !self.load(&mut cache)?.secrets.contains_key(&account);
        self.store(&mut cache, account.clone(), Some(value.clone()))?;
        if unknown {
            // Puede haber una entrada anterior con el valor viejo. Si no se la puede borrar no
            // importa: ya no se la lee, y `delete` lo vuelve a intentar.
            let _ = self.entries.remove(&account);
        }
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError> {
        let account = key.account();
        let mut cache = self.lock();
        if let Some(known) = self.load(&mut cache)?.secrets.get(&account) {
            return Ok(known.clone());
        }
        // Primera vez que se pide: puede estar en una entrada anterior.
        let previous = self.entries.read(&account)?;
        // Si no se puede guardar, el secreto sirve igual: la entrada anterior queda como está y
        // la migración se reintenta en el próximo pedido.
        if self
            .store(&mut cache, account.clone(), previous.clone())
            .is_ok()
            && previous.is_some()
        {
            // Borrarla es limpieza: si falla, el valor ya está en la entrada nueva.
            let _ = self.entries.remove(&account);
        }
        Ok(previous)
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        let account = key.account();
        let mut cache = self.lock();
        if !matches!(self.load(&mut cache)?.secrets.get(&account), Some(None)) {
            self.store(&mut cache, account.clone(), None)?;
        }
        // Siempre: puede quedar la entrada anterior, y quien borra quiere que no quede nada.
        self.entries.remove(&account)
    }
}

#[cfg(test)]
mod tests {
    use openlight_types::DeviceId;

    use super::*;
    use crate::entries::fake::{Accesses, FakeEntries};

    fn device(n: u8) -> SecretKey {
        SecretKey::DeviceLocalKey(DeviceId(format!("bf000000000000000fake{n}")))
    }

    /// Las cuatro entradas que deja la versión anterior con la nube y dos focos.
    fn previous() -> Vec<(SecretKey, &'static str)> {
        vec![
            (SecretKey::CloudAccessId, "id-falso"),
            (SecretKey::CloudAccessSecret, "secreto-falso"),
            (device(1), "clave-falsa-\"1\\"),
            (device(2), "clave-falsa-ñ2"),
        ]
    }

    fn previous_keychain() -> FakeEntries {
        let keychain = FakeEntries::default();
        for (key, value) in previous() {
            keychain.put(&key.account(), value);
        }
        keychain
    }

    fn value(store: &dyn SecretStore, key: &SecretKey) -> Option<String> {
        store.get(key).unwrap().map(Secret::into_inner)
    }

    fn secret(value: &str) -> Secret<String> {
        Secret::new(value.to_owned())
    }

    #[test]
    fn cumple_el_contrato() {
        crate::contract_suite(&Vault::new(FakeEntries::default()));
    }

    #[test]
    fn todo_queda_en_una_sola_entrada() {
        let keychain = FakeEntries::default();
        let vault = Vault::new(keychain.clone());
        for (key, value) in previous() {
            vault.set(&key, &secret(value)).unwrap();
        }
        assert_eq!(keychain.accounts(), [VAULT_ACCOUNT]);
    }

    #[test]
    fn una_sola_lectura_del_llavero_por_arranque() {
        let keychain = FakeEntries::default();
        let first = Vault::new(keychain.clone());
        for (key, value) in previous() {
            first.set(&key, &secret(value)).unwrap();
        }

        keychain.reset_accesses();
        let reopened = Vault::new(keychain.clone());
        assert_eq!(keychain.accesses(), Accesses::default(), "crearlo no lee");
        for _ in 0..3 {
            for (key, expected) in previous() {
                assert_eq!(value(&reopened, &key).as_deref(), Some(expected));
            }
        }
        assert_eq!(
            keychain.accesses(),
            Accesses {
                reads: 1,
                writes: 0,
                removes: 0
            }
        );
    }

    #[test]
    fn pedidos_a_la_vez_leen_el_llavero_una_sola_vez() {
        let keychain = FakeEntries::default();
        Vault::new(keychain.clone())
            .set(&device(1), &secret("clave-falsa"))
            .unwrap();

        keychain.reset_accesses();
        let vault = Vault::new(keychain.clone());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    assert_eq!(value(&vault, &device(1)).as_deref(), Some("clave-falsa"));
                });
            }
        });
        assert_eq!(keychain.accesses().reads, 1);
    }

    #[test]
    fn migra_las_entradas_anteriores() {
        let keychain = previous_keychain();
        let vault = Vault::new(keychain.clone());
        for (key, expected) in previous() {
            assert_eq!(value(&vault, &key).as_deref(), Some(expected));
        }
        assert_eq!(
            keychain.accounts(),
            [VAULT_ACCOUNT],
            "las anteriores se borran"
        );
        assert_eq!(
            keychain.accesses().reads,
            5,
            "la entrada nueva y, una vez, cada anterior"
        );

        keychain.reset_accesses();
        let reopened = Vault::new(keychain.clone());
        for (key, expected) in previous() {
            assert_eq!(value(&reopened, &key).as_deref(), Some(expected));
        }
        assert_eq!(
            keychain.accesses(),
            Accesses {
                reads: 1,
                writes: 0,
                removes: 0
            }
        );
    }

    #[test]
    fn lo_que_no_existe_se_busca_una_sola_vez_entre_arranques() {
        let keychain = FakeEntries::default();
        assert_eq!(
            value(&Vault::new(keychain.clone()), &SecretKey::CloudAccessId),
            None
        );

        keychain.reset_accesses();
        let reopened = Vault::new(keychain.clone());
        assert_eq!(value(&reopened, &SecretKey::CloudAccessId), None);
        assert_eq!(value(&reopened, &SecretKey::CloudAccessId), None);
        assert_eq!(
            keychain.accesses(),
            Accesses {
                reads: 1,
                writes: 0,
                removes: 0
            }
        );
    }

    #[test]
    fn borrar_no_deja_ni_resucita_la_entrada_anterior() {
        let keychain = previous_keychain();
        let vault = Vault::new(keychain.clone());
        // Sin haberla leído antes
        vault.delete(&device(1)).unwrap();
        assert_eq!(keychain.item(&device(1).account()), None);
        assert_eq!(value(&vault, &device(1)), None);

        // Aunque alguien la vuelva a crear, el borrado gana
        keychain.put(&device(1).account(), "clave-vieja");
        assert_eq!(value(&Vault::new(keychain.clone()), &device(1)), None);
    }

    #[test]
    fn guardar_reemplaza_a_la_entrada_anterior_y_la_borra() {
        let keychain = previous_keychain();
        let vault = Vault::new(keychain.clone());
        vault.set(&device(1), &secret("clave-nueva")).unwrap();
        assert_eq!(keychain.item(&device(1).account()), None);
        assert_eq!(value(&vault, &device(1)).as_deref(), Some("clave-nueva"));
        assert_eq!(
            value(&Vault::new(keychain.clone()), &device(1)).as_deref(),
            Some("clave-nueva")
        );
    }

    #[test]
    fn si_no_se_puede_leer_no_se_pisa_nada_y_despues_se_reintenta() {
        let keychain = FakeEntries::default();
        Vault::new(keychain.clone())
            .set(&device(1), &secret("clave-falsa"))
            .unwrap();
        let saved = keychain.item(VAULT_ACCOUNT);

        keychain.reset_accesses();
        keychain.deny_reads(true);
        let vault = Vault::new(keychain.clone());
        assert!(vault.get(&device(1)).is_err());
        assert!(vault.set(&device(2), &secret("otra")).is_err());
        assert!(vault.delete(&device(1)).is_err());
        assert_eq!(keychain.accesses().writes, 0);
        assert_eq!(keychain.accesses().removes, 0);
        assert_eq!(keychain.item(VAULT_ACCOUNT), saved);

        keychain.deny_reads(false);
        assert_eq!(value(&vault, &device(1)).as_deref(), Some("clave-falsa"));
    }

    #[test]
    fn si_falla_la_escritura_al_migrar_la_entrada_anterior_queda() {
        let keychain = previous_keychain();
        keychain.deny_writes(true);
        let vault = Vault::new(keychain.clone());
        // La clave sirve igual, y la entrada anterior sigue siendo la única copia
        assert_eq!(
            value(&vault, &SecretKey::CloudAccessId).as_deref(),
            Some("id-falso")
        );
        assert_eq!(
            keychain.item("cloud:access_id").as_deref(),
            Some("id-falso")
        );
        assert_eq!(keychain.accesses().removes, 0);

        keychain.deny_writes(false);
        assert_eq!(
            value(&vault, &SecretKey::CloudAccessId).as_deref(),
            Some("id-falso")
        );
        assert_eq!(keychain.item("cloud:access_id"), None);
        assert!(keychain.item(VAULT_ACCOUNT).is_some());
    }

    #[test]
    fn si_falla_la_escritura_al_guardar_queda_lo_de_antes() {
        let keychain = FakeEntries::default();
        let vault = Vault::new(keychain.clone());
        vault.set(&device(1), &secret("clave-vieja")).unwrap();
        keychain.deny_writes(true);
        assert!(vault.set(&device(1), &secret("clave-nueva")).is_err());
        assert!(vault.delete(&device(1)).is_err());
        assert_eq!(value(&vault, &device(1)).as_deref(), Some("clave-vieja"));
    }

    #[test]
    fn un_borrado_que_el_llavero_rechaza_se_informa_y_no_se_revive() {
        let keychain = previous_keychain();
        keychain.deny_removes(true);
        let vault = Vault::new(keychain.clone());
        assert!(vault.delete(&device(1)).is_err());
        assert_eq!(value(&vault, &device(1)), None);

        keychain.deny_removes(false);
        vault.delete(&device(1)).unwrap();
        assert_eq!(keychain.item(&device(1).account()), None);
    }

    #[test]
    fn un_contenido_que_no_se_entiende_no_se_filtra_ni_se_pisa() {
        for contents in [
            "valor-secreto",
            r#"{"version":1,"secrets":"valor-secreto"}"#,
            r#"{"version":2,"secrets":{}}"#,
        ] {
            let keychain = FakeEntries::with(&[(VAULT_ACCOUNT, contents)]);
            let vault = Vault::new(keychain.clone());
            let error = vault.get(&device(1)).unwrap_err();
            assert!(!format!("{error} {error:?}").contains("valor-secreto"));
            assert!(vault.set(&device(1), &secret("clave")).is_err());
            assert_eq!(keychain.item(VAULT_ACCOUNT).as_deref(), Some(contents));
        }
    }
}
