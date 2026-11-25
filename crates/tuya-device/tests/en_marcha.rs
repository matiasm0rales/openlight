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
