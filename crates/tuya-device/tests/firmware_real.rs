//! El actor contra un foco que se porta como el W509Z2 real (`real_firmware` de `tuya-sim`):
//! seqno propio y `CONTROL` contestado solo con el push de `STATUS`, sin ack. Es el bug que
//! apareció en la prueba I2: `restore()` hacía que el foco aplicara el color original pero la
//! app devolvía `device_timeout`.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::*;
use openlight_types::{DeviceSessionState, Hsv, ProtocolVersion};
use serde_json::json;
use tuya_sim::SimConfig;

async fn rig_real(version: ProtocolVersion) -> Rig {
    let sim = tuya_sim::start(SimConfig::new(version).ephemeral().real_firmware(true))
        .await
        .unwrap();
    // El foco estaba en blanco antes de empezar.
    sim.set_dps(estado_blanco());
    let rig = Rig::with(sim, |c| c);
    rig.start().await;
    rig.sim.reset_stats();
    rig
}

fn volvio_a_blanco(rig: &Rig) -> bool {
    estado_blanco()
        .iter()
        .all(|(dp, v)| rig.sim.dp(dp).as_ref() == Some(v))
}

#[test]
fn prueba_rgb_y_restaurar() {
    run(async {
        for version in ProtocolVersion::ALL {
            let rig = rig_real(version).await;
            rig.activate().await;
            // La prueba RGB: tres colores sin esperar respuesta.
            for h in [0.0, 1.0 / 3.0, 2.0 / 3.0] {
                rig.device.send_colour(Hsv::new(h, 1.0, 1.0));
                pump(Duration::from_millis(50)).await;
            }
            assert_eq!(rig.sim.dp("21"), Some(json!("colour")), "{version}");

            // Y salir: el control con espera se confirma con el push, aunque haya pushes de la
            // prueba RGB sin leer.
            drive(rig.device.restore())
                .await
                .unwrap_or_else(|e| panic!("{version}: restore: {e}"));
            pump(STEP).await;
            assert!(volvio_a_blanco(&rig), "{version}: {:?}", rig.sim.dps());
            assert_eq!(rig.store_original(), None, "{version}: olvida lo guardado");
        }
    });
}

#[test]
fn sesion_completa_con_pausa_y_retoma() {
    run(async {
        let rig = rig_real(ProtocolVersion::V35).await;
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.3, 1.0, 1.0));
        pump(STEP).await;
        // Pausa sin pantalla: devuelve el estado y sigue conectado.
        drive(rig.device.return_to_original()).await.expect("pausa");
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig));
        assert!(rig.device.health().connected);
        // Retoma.
        rig.device.activate().await.unwrap();
        rig.device.send_colour(Hsv::new(0.5, 1.0, 1.0));
        pump(STEP).await;
        assert_eq!(rig.sim.dp("24"), Some(json!("00b403e803e8")));
        assert_eq!(rig.device.snapshot().state, DeviceSessionState::Active);
        drive(rig.device.restore()).await.expect("sale");
        assert!(volvio_a_blanco(&rig));
    });
}

#[test]
fn status_set_dps_y_latido_con_espera() {
    run(async {
        let rig = rig_real(ProtocolVersion::V35).await;
        let status = drive(rig.device.status()).await.expect("status");
        assert_eq!(status["21"], json!("white"));
        drive(rig.device.set_dps(dps(json!({"22": 300})), true))
            .await
            .expect("set con espera");
        assert_eq!(rig.sim.dp("22"), Some(json!(300)));
        drive(rig.device.heartbeat())
            .await
            .expect("latido con espera");
        assert!(rig.device.health().rtt_ms.is_some());
        // El latido periódico sigue andando y la conexión no se corta.
        pump_slow(Duration::from_secs(60)).await;
        assert!(rig.device.health().connected);
        assert_eq!(rig.device.health().disconnects_last_hour, 0);
    });
}

#[test]
fn restaurar_sin_cambios_no_manda_nada_ni_espera() {
    run(async {
        let rig = rig_real(ProtocolVersion::V35).await;
        // Nunca se activó: el foco sigue como estaba, así que no hay nada que devolver.
        drive(rig.device.restore()).await.expect("restaura");
        assert!(rig.writes().is_empty(), "{:?}", rig.writes());
        assert_eq!(rig.store_original(), None);
    });
}

#[test]
fn restaurar_despues_de_un_corte_con_el_firmware_real() {
    run(async {
        let rig = rig_real(ProtocolVersion::V35).await;
        rig.activate().await;
        rig.sim.disconnect_all();
        pump(STEP).await;
        drive(rig.device.restore()).await.expect("restaura");
        pump(STEP).await;
        assert!(volvio_a_blanco(&rig), "{:?}", rig.sim.dps());
    });
}

#[test]
fn set_dps_con_espera_de_lo_que_el_foco_ya_tiene_no_corta_la_conexion() {
    run(async {
        let rig = rig_real(ProtocolVersion::V35).await;
        // El foco ya está en blanco: no hay push, pero el pedido está cumplido.
        drive(rig.device.set_dps(dps(json!({"21": "white"})), true))
            .await
            .expect("ya estaba así");
        assert!(rig.device.health().connected);
        assert_eq!(rig.device.health().disconnects_last_hour, 0);
    });
}
