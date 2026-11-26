//! Las entradas del llavero tal como las ve el sistema: un texto por nombre de cuenta.
//!
//! Es la costura entre el llavero real y las pruebas: [`crate::vault::Vault`] decide qué se
//! guarda y dónde, y acá solo se lee, escribe y borra.

use openlight_types::Secret;

use crate::SecretError;

/// Entradas del llavero bajo el servicio de OpenLight.
pub(crate) trait Entries: Send + Sync {
    /// El contenido de una entrada, o `None` si no existe.
    fn read(&self, account: &str) -> Result<Option<Secret<String>>, SecretError>;
    /// Crea o reemplaza una entrada.
    fn write(&self, account: &str, value: &Secret<String>) -> Result<(), SecretError>;
    /// Borra una entrada. Borrar una que no existe no es un error.
    fn remove(&self, account: &str) -> Result<(), SecretError>;
}

/// Un llavero falso, para las pruebas: cuenta los accesos y puede fallar a pedido.
#[cfg(test)]
pub(crate) mod fake {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Accesses {
        pub reads: usize,
        pub writes: usize,
        pub removes: usize,
    }

    #[derive(Default)]
    struct State {
        items: BTreeMap<String, String>,
        accesses: Accesses,
        deny_reads: bool,
        deny_writes: bool,
        deny_removes: bool,
    }

    /// Los clones comparten el contenido: un clon nuevo hace de "la app que vuelve a abrir".
    #[derive(Default, Clone)]
    pub(crate) struct FakeEntries(Arc<Mutex<State>>);

    impl FakeEntries {
        /// Un llavero que ya tiene estas entradas.
        pub(crate) fn with(items: &[(&str, &str)]) -> Self {
            let fake = Self::default();
            for (account, value) in items {
                fake.put(account, value);
            }
            fake
        }

        pub(crate) fn put(&self, account: &str, value: &str) {
            self.state()
                .items
                .insert(account.to_owned(), value.to_owned());
        }

        pub(crate) fn item(&self, account: &str) -> Option<String> {
            self.state().items.get(account).cloned()
        }

        /// Los nombres de las entradas que existen.
        pub(crate) fn accounts(&self) -> Vec<String> {
            self.state().items.keys().cloned().collect()
        }

        pub(crate) fn accesses(&self) -> Accesses {
            self.state().accesses
        }

        pub(crate) fn reset_accesses(&self) {
            self.state().accesses = Accesses::default();
        }

        pub(crate) fn deny_reads(&self, deny: bool) {
            self.state().deny_reads = deny;
        }

        pub(crate) fn deny_writes(&self, deny: bool) {
            self.state().deny_writes = deny;
        }

        pub(crate) fn deny_removes(&self, deny: bool) {
            self.state().deny_removes = deny;
        }

        fn state(&self) -> std::sync::MutexGuard<'_, State> {
            self.0.lock().unwrap()
        }
    }

    fn denied(account: &str) -> SecretError {
        SecretError::Backend {
            entry: account.to_owned(),
            detail: "denegado".to_owned(),
        }
    }

    impl Entries for FakeEntries {
        fn read(&self, account: &str) -> Result<Option<Secret<String>>, SecretError> {
            let mut state = self.state();
            state.accesses.reads += 1;
            if state.deny_reads {
                return Err(denied(account));
            }
            Ok(state.items.get(account).cloned().map(Secret::new))
        }

        fn write(&self, account: &str, value: &Secret<String>) -> Result<(), SecretError> {
            let mut state = self.state();
            state.accesses.writes += 1;
            if state.deny_writes {
                return Err(denied(account));
            }
            state
                .items
                .insert(account.to_owned(), value.expose().clone());
            Ok(())
        }

        fn remove(&self, account: &str) -> Result<(), SecretError> {
            let mut state = self.state();
            state.accesses.removes += 1;
            if state.deny_removes {
                return Err(denied(account));
            }
            state.items.remove(account);
            Ok(())
        }
    }
}
