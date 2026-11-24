//! `StateStore` de `tuya-device` sobre el `state.json` del `Store` de `openlight-core`.

use std::net::IpAddr;

use openlight_core::Store;
use openlight_types::DeviceId;
use tuya_device::{SavedState, StateStore};

/// Guarda la última IP y el estado original de cada foco en `state.json`
/// (`leer_estado()/guardar_estado()` del script).
#[derive(Debug, Clone)]
pub struct CoreStateStore {
    store: Store,
}

impl CoreStateStore {
    /// Sobre la carpeta de datos `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl StateStore for CoreStateStore {
    fn load(&self, id: &DeviceId) -> SavedState {
        match self.store.load_state() {
            Ok(state) => state
                .devices
                .get(id)
                .map(|d| SavedState {
                    ip: d
                        .last_ip
                        .as_deref()
                        .and_then(|ip| ip.parse::<IpAddr>().ok()),
                    version: d.version,
                    original: d.original.clone(),
                })
                .unwrap_or_default(),
            Err(error) => {
                tracing::warn!(device = %id, %error, "no se pudo leer state.json");
                SavedState::default()
            }
        }
    }

    fn save(&self, id: &DeviceId, state: &SavedState) {
        let result = self.store.update_state(|all| {
            let entry = all.devices.entry(id.clone()).or_default();
            entry.last_ip = state.ip.map(|ip| ip.to_string());
            entry.version = state.version;
            entry.original = state.original.clone();
        });
        if let Err(error) = result {
            tracing::warn!(device = %id, %error, "no se pudo guardar state.json");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::Ipv4Addr;

    use openlight_types::ProtocolVersion;
    use serde_json::json;

    use super::*;

    fn dev(id: &str) -> DeviceId {
        DeviceId(id.into())
    }

    #[test]
    fn guarda_y_lee_ip_y_original_en_state_json() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let adapter = CoreStateStore::new(store.clone());
        assert_eq!(adapter.load(&dev("a")), SavedState::default());

        let original = BTreeMap::from([("20".to_owned(), json!(true))]);
        let saved = SavedState {
            ip: Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 7))),
            version: Some(ProtocolVersion::V35),
            original: Some(original.clone()),
        };
        adapter.save(&dev("a"), &saved);
        assert_eq!(adapter.load(&dev("a")), saved);
        // Es lo mismo que ve el resto de la app
        let state = store.load_state().unwrap();
        assert_eq!(
            state.devices[&dev("a")].last_ip.as_deref(),
            Some("192.0.2.7")
        );
        assert_eq!(state.devices[&dev("a")].version, Some(ProtocolVersion::V35));

        // Restaurar borra el original pero recuerda la IP y la versión
        adapter.save(
            &dev("a"),
            &SavedState {
                original: None,
                ..saved
            },
        );
        assert_eq!(adapter.load(&dev("a")).original, None);
        assert!(adapter.load(&dev("a")).ip.is_some());
        assert_eq!(adapter.load(&dev("a")).version, Some(ProtocolVersion::V35));
    }

    #[test]
    fn un_state_json_ilegible_no_rompe_al_foco() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        std::fs::write(dir.path().join("state.json"), "no es json").unwrap();
        assert_eq!(
            CoreStateStore::new(store).load(&dev("a")),
            SavedState::default()
        );
    }
}
