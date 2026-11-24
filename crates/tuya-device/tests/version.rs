//! El actor adopta la versión del protocolo que anuncia el foco (como `auto_ip` de tinytuya):
//! un registro guardado como 3.3 para un foco 3.5 se corrige solo y se recuerda.
#![allow(missing_docs)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use openlight_types::ProtocolVersion;
use tuya_device::{IpMode, SavedState, StateStore, spawn};
use tuya_sim::SimConfig;

/// Un foco simulado que habla `real`, registrado como `registered` y que discovery anuncia
/// como `real`.
async fn rig_registrado_como(registered: ProtocolVersion, real: ProtocolVersion) -> Rig {
    let rig = Rig::with(sim(real).await, |mut c| {
        c.creds.version = registered;
        c
    });
    rig.locator.answer_version(LOCALHOST, real);
    rig
}

#[test]
fn un_registro_con_otra_version_se_corrige_y_se_guarda() {
    run(async {
        // Todas las combinaciones: registrado como una versión, el foco habla otra.
        for registered in ProtocolVersion::ALL {
            for real in ProtocolVersion::ALL {
                if registered == real {
                    continue;
                }
                let rig = rig_registrado_como(registered, real).await;
                let info = drive(rig.device.start())
                    .await
                    .unwrap_or_else(|e| panic!("registrado {registered}, real {real}: {e}"));
                assert_eq!(info.version, real, "registrado {registered}");
                assert!(rig.locator.calls() >= 1, "{registered} → {real}: buscó");
                let saved = rig.store.load(&rig.id);
                assert_eq!(saved.version, Some(real), "{registered} → {real}");
                assert_eq!(saved.ip, Some(LOCALHOST));
                assert!(rig.device.health().connected);
            }
        }
    });
}

#[test]
fn la_version_guardada_se_usa_sin_buscar_la_proxima_vez() {
    run(async {
        let rig = rig_registrado_como(ProtocolVersion::V33, ProtocolVersion::V35).await;
        rig.start().await;
        let saved = rig.store.load(&rig.id);
        assert_eq!(saved.version, Some(ProtocolVersion::V35));

        // Otra ejecución: el registro sigue diciendo 3.3 y discovery no encuentra nada.
        let locator = MockLocator::new(None);
        let mut config = config_for(&rig.sim);
        config.creds.version = ProtocolVersion::V33;
        let device = spawn(config, locator.clone(), rig.store.clone());
        let info = drive(device.start())
            .await
            .expect("arranca con la versión guardada");
        assert_eq!(info.version, ProtocolVersion::V35);
        assert_eq!(locator.calls(), 0, "no hizo falta buscar");
    });
}

#[test]
fn con_ip_fija_no_se_adopta_nada() {
    run(async {
        let rig = Rig::with(sim(ProtocolVersion::V35).await, |mut c| {
            c.creds.version = ProtocolVersion::V33;
            c.ip_mode(IpMode::Fixed)
        });
        rig.locator.answer_version(LOCALHOST, ProtocolVersion::V35);
        let result = drive(rig.device.start()).await;
        assert!(result.is_err(), "{result:?}");
        assert_eq!(rig.locator.calls(), 0, "con IP fija no hay discovery");
        assert_eq!(rig.store.load(&rig.id).version, None);
    });
}

#[test]
fn si_anuncia_la_misma_version_no_se_guarda_nada() {
    run(async {
        let rig = Rig::with(sim(ProtocolVersion::V35).await, |mut c| {
            c.creds.ip = Some("192.0.2.1".parse().unwrap());
            c
        });
        rig.locator.answer_version(LOCALHOST, ProtocolVersion::V35);
        let info = rig.start().await;
        assert_eq!(info.version, ProtocolVersion::V35);
        let saved = rig.store.load(&rig.id);
        assert_eq!(saved.ip, Some(LOCALHOST));
        assert_eq!(saved.version, None, "no difiere de la del registro");
    });
}

#[test]
fn un_locator_que_no_sabe_la_version_no_la_cambia() {
    run(async {
        let rig = Rig::with(sim(ProtocolVersion::V35).await, |mut c| {
            c.creds.ip = Some("192.0.2.1".parse().unwrap());
            c
        });
        // Solo la IP (el `MockLocator::new` de siempre).
        let info = rig.start().await;
        assert_eq!(info.version, ProtocolVersion::V35);
        assert_eq!(rig.store.load(&rig.id).version, None);
    });
}

#[test]
fn una_version_vieja_guardada_se_olvida_si_el_foco_vuelve_a_la_del_registro() {
    run(async {
        let sim = sim(ProtocolVersion::V35).await;
        let id = sim.dev_id().clone();
        // Quedó guardado 3.3 de otra vez (y una IP que ya no sirve).
        let store = Arc::new(tuya_device::MemoryStore::new());
        store.save(
            &id,
            &SavedState {
                ip: Some("192.0.2.1".parse().unwrap()),
                version: Some(ProtocolVersion::V33),
                original: None,
            },
        );
        // El registro dice 3.5 y ahora el foco también anuncia 3.5.
        let locator = MockLocator::announcing(LOCALHOST, ProtocolVersion::V35);
        let mut config = config_with(id.clone(), ProtocolVersion::V35, sim.port());
        config.creds.ip = None;
        let device = spawn(config, locator, store.clone());
        let info = drive(device.start()).await.expect("arranca");
        assert_eq!(info.version, ProtocolVersion::V35);
        assert_eq!(store.load(&id).version, None);
        assert_eq!(store.load(&id).ip, Some(LOCALHOST));
    });
}

#[test]
fn al_reconectar_tambien_adopta_la_version() {
    run(async {
        let first = sim(ProtocolVersion::V35).await;
        let port = first.port();
        let store = Arc::new(tuya_device::MemoryStore::new());
        let locator = MockLocator::announcing(LOCALHOST, ProtocolVersion::V35);
        let id = first.dev_id().clone();
        let device = spawn(
            config_with(id.clone(), ProtocolVersion::V35, port),
            locator.clone(),
            store.clone(),
        );
        drive(device.start()).await.expect("arranca");

        // El foco se actualiza y vuelve hablando 3.4 en la misma IP y puerto.
        first.shutdown().await;
        let second = tuya_sim::start(SimConfig::new(ProtocolVersion::V34).bind(addr(port)))
            .await
            .expect("el foco vuelve");
        locator.answer_version(LOCALHOST, ProtocolVersion::V34);
        pump_slow(Duration::from_secs(60)).await;

        let health = device.health();
        assert!(health.connected, "{health:?}");
        assert_eq!(health.reconnects, 1);
        assert_eq!(store.load(&id).version, Some(ProtocolVersion::V34));
        assert_eq!(second.stats().negotiations, 1);
    });
}
