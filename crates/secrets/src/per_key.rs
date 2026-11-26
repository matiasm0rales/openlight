//! Una entrada del llavero por secreto.

use openlight_types::Secret;

use crate::entries::Entries;
use crate::{SecretError, SecretKey, SecretStore};

/// Cada secreto en su entrada, con el nombre de [`SecretKey::account`].
pub(crate) struct PerKey<E>(pub(crate) E);

impl<E: Entries> SecretStore for PerKey<E> {
    fn set(&self, key: &SecretKey, value: &Secret<String>) -> Result<(), SecretError> {
        self.0.write(&key.account(), value)
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret<String>>, SecretError> {
        self.0.read(&key.account())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.0.remove(&key.account())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entries::fake::FakeEntries;

    #[test]
    fn cumple_el_contrato() {
        crate::contract_suite(&PerKey(FakeEntries::default()));
    }
}
