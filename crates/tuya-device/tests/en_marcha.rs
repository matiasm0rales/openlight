//! Ajustes que cambian con el actor andando: canal del color y tiempos de la conexión.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::*;
use openlight_types::{Hsv, ProtocolVersion, SendChannel, Settings};
use serde_json::json;
use tuya_device::LinkSettings;

fn verde() -> Hsv {
    Hsv::new(124.0 / 360.0, 0.316, 0.728)
}

#[test]
fn el_canal_cambia_sin_reconectar() {
    run(async {
        let sim = sim(ProtocolVersion::V35).await;
        let caps = caps_of(&sim);
        let rig = Rig::with(sim, |c| c.capabilities(caps));
        rig.start().await;
        rig.activate().await;
        rig.device.send_colour(verde());
        pump(STEP).await;

        rig.device.configure(LinkSettings::from_settings(&Settings {
            send_channel: SendChannel::ControlDirect,
            ..Settings::default()
        }));
        rig.device.send_colour(verde());
        pump(STEP).await;
        assert_eq!(
            rig.writes(),
            vec![
                dps(json!({"24": "007c013c02d8"})),
                dps(json!({"28": "0007c013c02d800000000"})),
            ]
        );
        assert_eq!(rig.sim.stats().connections, 0, "no volvió a conectar");
    });
}

#[test]
fn el_latido_nuevo_vale_enseguida() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.device.configure(LinkSettings::from_settings(&Settings {
            heartbeat_s: 3.0,
            ..Settings::default()
        }));
        // Con 10 s (el defecto) en 7 s no saldría ninguno; con 3 s salen dos
        pump_slow(Duration::from_secs(7)).await;
        assert_eq!(rig.sim.stats().heartbeats(), 2);
        assert_eq!(rig.sim.stats().connections, 1);
    });
}

#[test]
fn el_plazo_de_conexion_muerta_nuevo_vale_enseguida() {
    run(async {
        // 3.3 para que un foco mudo no cuente como "clave rechazada" al reconectar.
        let rig = Rig::new(ProtocolVersion::V33).await;
        rig.start().await;
        rig.device.configure(LinkSettings::from_settings(&Settings {
            dead_after_s: 8.0,
            ..Settings::default()
        }));
        rig.sim.update_faults(|f| f.mute = true);
        pump(Duration::from_secs(7)).await;
        assert!(rig.device.health().connected);
        // Con 30 s (el defecto) seguiría "conectado"
        pump(Duration::from_secs(2)).await;
        assert!(!rig.device.health().connected);
    });
}

#[test]
fn la_espera_entre_reintentos_nueva_vale_enseguida() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.device.configure(LinkSettings::from_settings(&Settings {
            retry_every_s: 1.0,
            ..Settings::default()
        }));
        rig.sim.update_faults(|f| f.occupied = true);
        rig.sim.disconnect_all();
        pump(Duration::from_millis(900)).await;
        assert_eq!(rig.sim.stats().rejected, 0);
        // Con 3 s (el defecto) todavía no habría ningún intento
        pump(Duration::from_millis(300)).await;
        assert_eq!(rig.sim.stats().rejected, 1);
    });
}
