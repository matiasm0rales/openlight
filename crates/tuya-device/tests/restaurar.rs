//! Port de `FocoRestaurar` de `test_ambilight.py` y de las pruebas de pausa de `FocoConexion`.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::*;
use openlight_types::{DeviceError, DeviceSessionState, Hsv, ProtocolVersion};
use serde_json::json;
use tuya_device::{SavedState, StateStore};

/// Un rig con el foco en blanco (como estaba antes de arrancar) y ya iniciado.
async fn rig_en_blanco() -> Rig {
    let rig = Rig::new(ProtocolVersion::V35).await;
    rig.sim.set_dps(estado_blanco());
    rig.start().await;
    rig.sim.reset_stats();
    rig
}

fn volvio_a_blanco(rig: &Rig) -> bool {
    let blanco = estado_blanco();
    blanco
        .iter()
        .all(|(dp, v)| rig.sim.dp(dp).as_ref() == Some(v))
}

// port: test_ambilight.py::FocoRestaurar::test_devuelve_el_estado_y_olvida_el_guardado
#[test]
fn devuelve_el_estado_y_olvida_el_guardado() {
    run(async {
        let rig = rig_en_blanco().await;
        assert_eq!(rig.store.load(&rig.id).original, Some(estado_blanco()));
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.3, 1.0, 1.0));
        pump(STEP).await;
        assert_eq!(rig.sim.dp("21"), Some(json!("colour")));

        drive(rig.device.restore()).await.expect("restaura");
        pump(STEP).await;

        assert!(volvio_a_blanco(&rig), "{:?}", rig.sim.dps());
        let saved = rig.store.load(&rig.id);
        assert_eq!(saved.original, None, "olvida lo guardado");
        assert_eq!(saved.ip, Some(LOCALHOST), "pero recuerda la IP");
        assert_eq!(rig.sim.active_connections(), 0, "cierra la conexión");
        let snapshot = rig.device.snapshot();
        assert!(!snapshot.health.connected);
        assert_eq!(snapshot.state, DeviceSessionState::Idle);
        assert_eq!(
            snapshot.health.disconnects_last_hour, 0,
            "salir no es un corte"
        );
        // Ya no reconecta solo.
        let conexiones = rig.sim.stats().connections;
        pump_slow(Duration::from_secs(30)).await;
        assert_eq!(rig.sim.stats().connections, conexiones);
    });
}

// port: test_ambilight.py::FocoRestaurar::test_si_falla_conserva_lo_guardado_para_la_proxima
#[test]
fn si_falla_conserva_lo_guardado_para_la_proxima() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.store.save(
            &rig.id,
            &SavedState {
                ip: None,
                version: None,
                original: Some(estado_blanco()),
            },
        );
        rig.start().await;
        // El foco deja de contestar justo antes de salir.
        rig.sim.update_faults(|f| f.mute = true);
        let result = drive(rig.device.restore()).await;
        assert_eq!(result, Err(DeviceError::Timeout));
        assert_eq!(
            rig.store.load(&rig.id).original,
            Some(estado_blanco()),
            "queda guardado para la próxima ejecución"
        );
        pump(STEP).await;
        assert_eq!(
            rig.sim.active_connections(),
            0,
            "la conexión se cierra igual"
        );
    });
}

// port: test_ambilight.py::FocoConexion::test_en_pausa_reconectar_no_vuelve_al_modo_color
#[test]
fn en_pausa_reconectar_no_vuelve_al_modo_color() {
    run(async {
        let rig = rig_en_blanco().await;
        rig.activate().await;
        drive(rig.device.return_to_original()).await.expect("pausa");
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig));
        assert!(
            rig.device.health().connected,
            "la pausa no cierra la conexión"
        );
        rig.sim.reset_stats();

        // Se corta la conexión en plena pausa y se recupera.
        rig.sim.disconnect_all();
        pump_slow(Duration::from_secs(5)).await;
        assert_eq!(rig.device.health().reconnects, 1);
        assert!(
            rig.writes().is_empty(),
            "no vuelve al modo color: {:?}",
            rig.writes()
        );
        assert!(volvio_a_blanco(&rig));
    });
}

#[test]
fn al_retomar_despues_de_una_pausa_vuelve_a_seguir_colores() {
    run(async {
        let rig = rig_en_blanco().await;
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.2, 1.0, 1.0));
        pump(STEP).await;
        drive(rig.device.return_to_original()).await.unwrap();
        // Con la pantalla en pausa, los colores no llegan al foco.
        rig.device.send_colour(Hsv::new(0.9, 1.0, 1.0));
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig));
        // Y al volver la pantalla se retoma, sin el color viejo.
        rig.sim.reset_stats();
        rig.device.activate().await.unwrap();
        pump(STEP).await;
        rig.device.send_colour(Hsv::new(0.5, 1.0, 1.0));
        pump(STEP).await;
        assert_eq!(
            rig.writes(),
            vec![
                dps(json!({"20": true, "21": "colour"})),
                dps(json!({"24": "00b403e803e8"})),
            ]
        );
        assert_eq!(rig.device.snapshot().state, DeviceSessionState::Active);
    });
}

#[test]
fn restaurar_justo_despues_de_un_corte_reconecta_una_vez() {
    run(async {
        let rig = rig_en_blanco().await;
        rig.activate().await;
        rig.sim.disconnect_all();
        pump(STEP).await;
        assert!(!rig.device.health().connected);
        // No espera el backoff: sale ya, con un intento de reconexión propio.
        drive(rig.device.restore()).await.expect("restaura");
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig), "{:?}", rig.sim.dps());
        assert_eq!(rig.store.load(&rig.id).original, None);
    });
}

#[test]
fn restaurar_con_el_foco_apagado_falla_sin_colgarse() {
    run(async {
        let rig = rig_en_blanco().await;
        rig.sim.update_faults(|f| f.occupied = true);
        rig.sim.disconnect_all();
        pump(STEP).await;
        let t0 = tokio::time::Instant::now();
        let result = drive(rig.device.restore()).await;
        assert!(result.is_err(), "{result:?}");
        assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
        assert_eq!(rig.store.load(&rig.id).original, Some(estado_blanco()));
    });
}

#[test]
fn restaurar_dos_veces_o_sin_iniciar_no_hace_nada() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        drive(rig.device.restore()).await.expect("sin iniciar");
        assert_eq!(
            rig.sim.stats().connections,
            0,
            "no se conecta para restaurar"
        );

        rig.start().await;
        drive(rig.device.restore()).await.unwrap();
        pump(STEP).await;
        let writes = rig.writes().len();
        drive(rig.device.restore()).await.unwrap();
        assert_eq!(rig.writes().len(), writes, "la segunda no manda nada");
    });
}

#[test]
fn se_puede_volver_a_iniciar_despues_de_restaurar() {
    run(async {
        let rig = rig_en_blanco().await;
        rig.activate().await;
        drive(rig.device.restore()).await.unwrap();
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig));

        let info = rig.start().await;
        assert_eq!(
            info.original,
            estado_blanco(),
            "toma el estado actual como original"
        );
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.5, 1.0, 1.0));
        pump(STEP).await;
        assert_eq!(rig.sim.dp("24"), Some(json!("00b403e803e8")));
    });
}
