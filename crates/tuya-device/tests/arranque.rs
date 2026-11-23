//! Port de `FocoArranque` de `test_ambilight.py`, contra `tuya-sim` con fallas inyectadas.
#![allow(missing_docs)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use openlight_types::{
    DeviceError, DeviceSessionState, DisconnectReason, ErrorCode, ProtocolVersion,
};
use serde_json::json;
use tokio::time::Instant;
use tuya_device::{DeviceHandle, IpMode, SavedState, StateStore, spawn};
use tuya_sim::Faults;

// port: test_ambilight.py::FocoArranque::test_guarda_el_estado_original_y_la_ip
#[test]
fn guarda_el_estado_original_y_la_ip() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        let info = rig.start().await;
        let saved = rig.store.load(&rig.id);
        assert_eq!(saved.ip, Some(LOCALHOST));
        let original = saved.original.expect("se guardó el original");
        assert_eq!(original["21"], json!("colour"));
        assert_eq!(info.original, original);
        assert_eq!(info.ip, LOCALHOST);
        assert_eq!(rig.device.snapshot().state, DeviceSessionState::Idle);
        assert!(rig.device.health().connected);
    });
}

// port: test_ambilight.py::FocoArranque::test_tras_un_cierre_sucio_recupera_el_original_guardado
#[test]
fn tras_un_cierre_sucio_recupera_el_original_guardado() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.store.save(
            &rig.id,
            &SavedState {
                ip: None,
                original: Some(estado_blanco()),
            },
        );
        let info = rig.start().await;
        // El foco simulado arranca en modo color, como si la ejecución anterior hubiera muerto.
        assert_eq!(info.original, estado_blanco());
        assert_eq!(rig.store.load(&rig.id).original, Some(estado_blanco()));
    });
}

// port: test_ambilight.py::FocoArranque::test_espera_a_que_el_foco_aparezca
#[test]
fn espera_a_que_el_foco_aparezca() {
    run(async {
        let port = free_port();
        let id = tuya_sim::DEFAULT_DEV_ID.to_owned();
        let device = spawn(
            config_with(openlight_types::DeviceId(id), ProtocolVersion::V35, port),
            MockLocator::new(Some(LOCALHOST)),
            Arc::new(tuya_device::MemoryStore::new()),
        );
        let t0 = Instant::now();
        let (result, sim) = drive(async {
            tokio::join!(device.start(), async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                tuya_sim::start(tuya_sim::SimConfig::new(ProtocolVersion::V35).bind(addr(port)))
                    .await
                    .expect("el foco aparece")
            })
        })
        .await;
        result.expect("el foco apareció a tiempo");
        assert!(t0.elapsed() >= Duration::from_secs(5));
        assert_eq!(sim.stats().connections, 1);
    });
}

// port: test_ambilight.py::FocoArranque::test_se_rinde_si_el_foco_nunca_aparece
#[test]
fn se_rinde_si_el_foco_nunca_aparece() {
    run(async {
        let device = spawn(
            config_with(
                openlight_types::DeviceId(tuya_sim::DEFAULT_DEV_ID.to_owned()),
                ProtocolVersion::V35,
                free_port(),
            ),
            MockLocator::new(Some(LOCALHOST)),
            Arc::new(tuya_device::MemoryStore::new()),
        );
        let t0 = Instant::now();
        let result = drive(device.start()).await;
        assert_eq!(result.err(), Some(DeviceError::NotFound));
        // Espera `startup_wait` (45 s) y recién ahí se rinde.
        let waited = t0.elapsed();
        assert!(waited >= Duration::from_secs(45), "{waited:?}");
        assert!(waited < Duration::from_secs(50), "{waited:?}");
        assert_eq!(
            device.snapshot().state,
            DeviceSessionState::Error(ErrorCode::DeviceNotFound)
        );
    });
}

// port: test_ambilight.py::FocoArranque::test_clave_rechazada_busca_la_ip_antes_de_culpar_a_la_clave
#[test]
fn clave_rechazada_busca_la_ip_antes_de_culpar_a_la_clave() {
    run(async {
        // Lo que hay en la IP guardada rechaza la clave (otro aparato): hasta que discovery
        // "encuentra" el foco de verdad.
        let sim = sim_with(
            ProtocolVersion::V35,
            Faults {
                reject_key: true,
                ..Faults::default()
            },
        )
        .await;
        let rig = Rig::with(sim, |c| c);
        // El foco "se arregla" cuando preguntan por la IP, como si apareciera en otra.
        let sim = rig.sim.clone();
        rig.locator
            .on_locate(move || sim.update_faults(|f| f.reject_key = false));
        let result = drive(rig.device.start()).await;
        result.expect("tras buscar la IP, el foco acepta la clave");
        assert_eq!(rig.locator.calls(), 1, "buscó la IP una sola vez");
        assert_eq!(rig.store.load(&rig.id).ip, Some(LOCALHOST));
    });
}

// port: test_ambilight.py::FocoArranque::test_clave_rechazada_dos_veces_dice_como_renovarla
#[test]
fn clave_rechazada_dos_veces_dice_como_renovarla() {
    run(async {
        let sim = sim_with(
            ProtocolVersion::V35,
            Faults {
                reject_key: true,
                ..Faults::default()
            },
        )
        .await;
        let rig = Rig::with(sim, |c| c);
        let result = drive(rig.device.start()).await;
        assert_eq!(result.err(), Some(DeviceError::KeyRejected));
        assert_eq!(
            rig.locator.calls(),
            1,
            "buscó la IP antes de culpar a la clave"
        );
        assert_eq!(
            rig.sim.stats().key_rejections,
            2,
            "dos consultas, como en el script"
        );
        let snapshot = rig.device.snapshot();
        assert_eq!(
            snapshot.state,
            DeviceSessionState::Error(ErrorCode::DeviceKeyRejected)
        );
        assert_eq!(
            snapshot.health.last_disconnect,
            Some(DisconnectReason::KeyRejected)
        );
    });
}

// port: test_ambilight.py::FocoArranque::test_con_ip_fija_no_busca_otra
#[test]
fn con_ip_fija_no_busca_otra() {
    run(async {
        let sim = sim_with(
            ProtocolVersion::V35,
            Faults {
                reject_key: true,
                ..Faults::default()
            },
        )
        .await;
        let rig = Rig::with(sim, |c| c.ip_mode(IpMode::Fixed));
        let result = drive(rig.device.start()).await;
        assert_eq!(result.err(), Some(DeviceError::KeyRejected));
        assert_eq!(rig.locator.calls(), 0, "con IP fija no hay discovery");
        assert_eq!(rig.sim.stats().key_rejections, 1, "una sola consulta");
        assert_eq!(
            rig.store.load(&rig.id).ip,
            None,
            "no se guarda una IP que no sirvió"
        );
    });
}

#[test]
fn soltar_el_futuro_de_start_cancela_la_espera() {
    run(async {
        let device = spawn(
            config_with(
                openlight_types::DeviceId(tuya_sim::DEFAULT_DEV_ID.to_owned()),
                ProtocolVersion::V35,
                free_port(),
            ),
            MockLocator::new(Some(LOCALHOST)),
            Arc::new(tuya_device::MemoryStore::new()),
        );
        // Se cancela a los 3 s, bien antes de los 45 de espera.
        let waited =
            drive(async { tokio::time::timeout(Duration::from_secs(3), device.start()).await })
                .await;
        assert!(waited.is_err(), "sigue esperando al foco");
        pump(Duration::from_millis(100)).await;
        assert_eq!(device.snapshot().state, DeviceSessionState::Idle);
        let _: &DeviceHandle = &device;
    });
}

#[test]
fn un_enchufe_no_es_una_luz() {
    run(async {
        let mapping = serde_json::from_value(json!({
            "9": {"code": "countdown_1", "type": "Integer", "values": {"min": 0, "max": 100}},
            "101": {"code": "x", "type": "Integer", "values": {"min": 0, "max": 100}}
        }))
        .unwrap();
        let sim = tuya_sim::start(
            tuya_sim::SimConfig::new(ProtocolVersion::V35)
                .ephemeral()
                .profile(tuya_sim::Profile::Custom(mapping)),
        )
        .await
        .unwrap();
        let rig = Rig::with(sim, |c| c);
        let result = drive(rig.device.start()).await;
        assert!(
            matches!(result, Err(DeviceError::Unsupported(_))),
            "{result:?}"
        );
    });
}
