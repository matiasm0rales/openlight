//! Backend en memoria: para pruebas y para correr donde no hay llavero.

use std::collections::BTreeMap;
use std::sync::Mutex;

use openlight_types::Secret;

use crate::{SecretError, SecretKey, SecretStore};

/// Guarda los secretos en memoria. Se pierden al cerrar el programa.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<BTreeMap<SecretKey, Secret<String>>>,
}

impl MemoryStore {
    /// Un almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cuántos secretos hay guardados.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Si no hay ninguno.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<SecretKey, Secret<String>>> {
        // Un hilo que entró en pánico no deja el mapa a medias: seguimos con lo que haya.
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStore")
            .field("entries", &self.len())
            .finish()
    }
}

impl SecretStore for MemoryStore {
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError> {
        self.lock().insert(key.clone(), value.clone());
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError> {
        Ok(self.lock().get(key).cloned())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.lock().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumple_el_contrato() {
        let store = MemoryStore::new();
        assert!(store.is_empty());
        crate::contract_suite(&store);
        assert!(store.is_empty());
    }

    #[test]
    fn debug_no_muestra_secretos() {
        let store = MemoryStore::new();
        store
            .set(
                &SecretKey::CloudAccessSecret,
                &Secret::new("valor-falso".into()),
            )
            .unwrap();
        assert!(!format!("{store:?}").contains("valor-falso"));
    }
}
