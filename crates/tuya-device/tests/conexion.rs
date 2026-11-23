//! Port de `FocoConexion` de `test_ambilight.py`: lectura continua, latidos, conexión muerta,
//! reconexión con backoff y motivos de corte. Todo contra `tuya-sim` con fallas inyectadas.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::*;
use openlight_types::{DeviceSessionState, DisconnectReason, ErrorCode, Hsv, ProtocolVersion};
use serde_json::json;
use tuya_device::StateStore;
use tuya_sim::Faults;

const SECS: fn(u64) -> Duration = Duration::from_secs;

// port: test_ambilight.py::FocoConexion::test_lee_y_descarta_lo_que_manda_el_foco
#[test]
fn lee_y_descarta_lo_que_manda_el_foco() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        // Un foco charlatán: 200 cambios de estado, cada uno un push de STATUS.
        for i in 0..200 {
            rig.sim.set_dps(dps(json!({"22": 10 + (i % 900)})));
            if i % 20 == 0 {
                pump(STEP).await;
            }
        }
        pump(SECS(1)).await;
        let health = rig.device.health();
        assert!(health.bytes_rx >= 5000, "leyó {} bytes", health.bytes_rx);
        assert!(health.connected);
        assert_eq!(health.disconnects_last_hour, 0);
        // Y sigue contestando: nada quedó sin leer atascando el socket.
        assert!(drive(rig.device.status()).await.is_ok());
    });
}

// port: test_ambilight.py::FocoConexion::test_sin_respuestas_da_la_conexion_por_muerta
#[test]
fn sin_respuestas_da_la_conexion_por_muerta() {
    run(async {
        // 3.3 para que un foco mudo no cuente como "clave rechazada" al reconectar.
        let rig = Rig::new(ProtocolVersion::V33).await;
        rig.start().await;
        rig.sim.update_faults(|f| f.mute = true);
        pump(SECS(29)).await;
        assert!(rig.device.health().connected, "todavía no pasaron 30 s");
        pump(SECS(2)).await;
        let health = rig.device.health();
        assert!(!health.connected);
        assert_eq!(health.last_disconnect, Some(DisconnectReason::NoResponse));
        assert_eq!(health.disconnects_last_hour, 1);
        assert_eq!(
            rig.device.snapshot().state,
            DeviceSessionState::Reconnecting
        );
        // Los latidos salieron igual (a los 10 y 20 s) aunque nadie contestó.
        assert!(rig.sim.stats().muted >= 2);
    });
}

// port: test_ambilight.py::FocoConexion::test_recibir_datos_mantiene_viva_la_conexion
#[test]
fn recibir_datos_mantiene_viva_la_conexion() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        // Los latidos contestados cuentan como datos: pasan 5 veces `dead_after` sin cortes.
        pump_slow(SECS(150)).await;
        let health = rig.device.health();
        assert!(health.connected);
        assert_eq!(health.disconnects_last_hour, 0);
        assert_eq!(health.reconnects, 0);
        assert!(rig.sim.stats().heartbeats() >= 14, "{:?}", rig.sim.stats());
        let rtt = health.rtt_ms.expect("el latido mide la latencia");
        assert!((0.0..1000.0).contains(&rtt), "rtt = {rtt}");
    });
}

// port: test_ambilight.py::FocoConexion::test_detecta_que_el_foco_cerro_la_conexion
#[test]
fn detecta_que_el_foco_cerro_la_conexion() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.sim.disconnect_all();
        pump(STEP).await;
        let snapshot = rig.device.snapshot();
        assert!(!snapshot.health.connected);
        assert_eq!(
            snapshot.health.last_disconnect,
            Some(DisconnectReason::PeerClosed)
        );
        assert_eq!(snapshot.state, DeviceSessionState::Reconnecting);
        assert!(snapshot.health.last_disconnect_at_ms.is_some());
    });
}

// port: test_ambilight.py::FocoConexion::test_al_reconectar_vuelve_a_poner_el_modo_color
#[test]
fn al_reconectar_vuelve_a_poner_el_modo_color() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        // Se cortó la luz: el foco arranca en blanco.
        rig.sim.set_dps(dps(json!({"21": "white"})));
        rig.sim.disconnect_all();
        pump(SECS(4)).await;
        let health = rig.device.health();
        assert!(health.connected);
        assert_eq!(health.reconnects, 1);
        assert_eq!(rig.writes(), vec![dps(json!({"20": true, "21": "colour"}))]);
        assert_eq!(rig.sim.dp("21"), Some(json!("colour")));
        assert_eq!(rig.device.snapshot().state, DeviceSessionState::Active);
    });
}

// port: test_ambilight.py::FocoConexion::test_no_intenta_reconectar_en_cada_cuadro
#[test]
fn no_intenta_reconectar_en_cada_cuadro() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.sim.update_faults(|f| f.occupied = true);
        rig.sim.disconnect_all();
        // Antes de `retry_every` (3 s) no hay ni un intento.
        pump(Duration::from_millis(2900)).await;
        assert_eq!(rig.sim.stats().rejected, 0);
        // Y después, con backoff: intentos a los 3, 6, 12, 24 y 36 s.
        pump(Duration::from_millis(200)).await;
        assert_eq!(rig.sim.stats().rejected, 1);
        pump_slow(SECS(37)).await;
        assert_eq!(
            rig.sim.stats().rejected,
            5,
            "el backoff espacia los intentos: 3, 6, 12, 24, 36 s"
        );
    });
}

// port: test_ambilight.py::FocoConexion::test_un_intento_fallido_no_cuenta_como_reconexion
#[test]
fn un_intento_fallido_no_cuenta_como_reconexion() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.sim.update_faults(|f| f.occupied = true);
        rig.sim.disconnect_all();
        pump(SECS(8)).await;
        let health = rig.device.health();
        assert!(rig.sim.stats().rejected >= 1, "hubo intentos");
        assert_eq!(health.reconnects, 0);
        assert!(!health.connected);
        // Cuando el foco vuelve, ahora sí cuenta.
        rig.sim.update_faults(|f| f.occupied = false);
        pump_slow(SECS(15)).await;
        let health = rig.device.health();
        assert!(health.connected);
        assert_eq!(health.reconnects, 1);
        assert_eq!(health.disconnects_last_hour, 1);
    });
}

// port: test_ambilight.py::FocoConexion::test_reconexion_interna_de_tinytuya_tambien_pone_el_modo_color
#[test]
fn un_color_pedido_durante_el_corte_sale_despues_del_modo_color() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        rig.sim.disconnect_all();
        // Se pide un color justo cuando se corta: llega después de reconectar, y después de
        // volver a poner el modo color.
        rig.device.send_colour(Hsv::new(0.5, 1.0, 1.0));
        pump(SECS(4)).await;
        assert_eq!(rig.device.health().reconnects, 1);
        assert_eq!(
            rig.writes(),
            vec![
                dps(json!({"20": true, "21": "colour"})),
                dps(json!({"24": "00b403e803e8"})),
            ]
        );
    });
}

// port: test_ambilight.py::FocoConexion::test_sin_conexion_no_envia
#[test]
fn sin_conexion_no_envia() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        rig.sim.update_faults(|f| f.occupied = true);
        rig.sim.disconnect_all();
        pump(STEP).await;
        for i in 0..10u8 {
            rig.device
                .send_colour(Hsv::new(f32::from(i) / 10.0, 1.0, 1.0));
        }
        pump(SECS(5)).await;
        assert!(
            rig.writes().is_empty(),
            "sin conexión no sale nada: {:?}",
            rig.writes()
        );
        // Se junta en el último y sale al volver.
        rig.sim.update_faults(|f| f.occupied = false);
        pump_slow(SECS(15)).await;
        let writes = rig.writes();
        assert_eq!(writes.len(), 2, "{writes:?}");
        assert_eq!(writes[0], dps(json!({"20": true, "21": "colour"})));
        assert_eq!(writes[1], dps(json!({"24": "014403e803e8"})));
    });
}

// port: test_ambilight.py::FocoConexion::test_manda_senal_de_vida_cuando_toca
#[test]
fn manda_senal_de_vida_cuando_toca() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.sim.reset_stats();
        pump(SECS(9)).await;
        assert_eq!(rig.sim.stats().heartbeats(), 0, "faltan 10 s");
        pump(SECS(2)).await;
        assert_eq!(rig.sim.stats().heartbeats(), 1, "a los 10 s");
        pump(SECS(1)).await;
        assert_eq!(
            rig.sim.stats().heartbeats(),
            1,
            "no hay otro hasta los 20 s"
        );
        pump(SECS(9)).await;
        assert_eq!(rig.sim.stats().heartbeats(), 2);
    });
}

// port: test_ambilight.py::FocoConexion::test_recordar_ip_nueva_al_reconectar
#[test]
fn recuerda_la_ip_nueva_cuando_la_vieja_deja_de_servir() {
    run(async {
        let sim = sim(ProtocolVersion::V35).await;
        // La IP guardada es de otra red (192.0.2.0/24 está reservada para documentación).
        let rig = Rig::with(sim, |mut c| {
            c.creds.ip = Some("192.0.2.1".parse().unwrap());
            c
        });
        let info = rig.start().await;
        assert_eq!(info.ip, LOCALHOST);
        assert!(rig.locator.calls() >= 1, "buscó la IP nueva");
        assert_eq!(rig.store.load(&rig.id).ip, Some(LOCALHOST));
    });
}

#[test]
fn un_914_en_marcha_se_ve_y_se_recupera() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        // El foco se re-vincula: otra clave.
        rig.sim.update_faults(|f| f.reject_key = true);
        pump(SECS(10)).await;
        let snapshot = rig.device.snapshot();
        assert_eq!(
            snapshot.state,
            DeviceSessionState::Error(ErrorCode::DeviceKeyRejected)
        );
        assert_eq!(
            snapshot.health.last_disconnect,
            Some(DisconnectReason::KeyRejected)
        );
        assert!(
            rig.locator.calls() >= 1,
            "ante un fallo vuelve a buscar la IP"
        );
        // Vuelve la clave buena: se recupera solo y repone el modo color.
        rig.sim.update_faults(|f| f.reject_key = false);
        pump_slow(SECS(30)).await;
        let snapshot = rig.device.snapshot();
        assert_eq!(snapshot.state, DeviceSessionState::Active);
        assert!(snapshot.health.connected);
        assert_eq!(rig.sim.dp("21"), Some(json!("colour")));
    });
}

#[test]
fn los_cortes_periodicos_se_cuentan_con_su_motivo() {
    run(async {
        // Reproduce el log del script: el foco corta cada 20 s y la app reconecta.
        let sim = sim_with(
            ProtocolVersion::V35,
            Faults {
                drop_every: Some(SECS(20)),
                ..Faults::default()
            },
        )
        .await;
        let rig = Rig::with(sim, |c| c);
        rig.start().await;
        rig.activate().await;
        pump_slow(SECS(120)).await;
        let health = rig.device.health();
        assert!(health.reconnects >= 4, "{health:?}");
        assert_eq!(
            health.disconnects_last_hour,
            health.reconnects + u32::from(!health.connected)
        );
        assert_eq!(health.last_disconnect, Some(DisconnectReason::PeerClosed));
        assert!(rig.sim.stats().drops >= 4);
        assert!(
            rig.sim.active_connections() <= 1,
            "no quedan sockets viejos"
        );
    });
}

#[test]
fn un_actor_sin_handles_se_va() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        let Rig { sim, device, .. } = rig;
        assert_eq!(sim.active_connections(), 1);
        drop(device);
        pump(SECS(1)).await;
        assert_eq!(
            sim.active_connections(),
            0,
            "el actor cerró el socket al irse"
        );
    });
}

#[test]
fn los_suscriptores_se_enteran_de_los_cambios_pero_no_de_cada_byte() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        let mut rx = rig.device.subscribe();
        rx.borrow_and_update();

        // Un push de STATUS cambia solo los bytes contados: nadie se despierta.
        rig.sim.set_dps(dps(json!({"22": 700})));
        pump(STEP).await;
        assert!(!rx.has_changed().unwrap());
        assert!(
            rig.device.health().bytes_rx > 0,
            "pero el handle los ve al día"
        );

        // Un corte sí.
        rig.sim.disconnect_all();
        pump(STEP).await;
        assert!(rx.has_changed().unwrap());
        assert_eq!(
            rx.borrow_and_update().state,
            DeviceSessionState::Reconnecting
        );
    });
}
