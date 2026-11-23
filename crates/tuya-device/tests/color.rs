//! Envío de colores: canal, coalescencia, modo color y el trait `LightDevice`.
//! Port de las pruebas de envío de `FocoConexion` de `test_ambilight.py`.
#![allow(missing_docs)]

mod common;

use common::*;
use openlight_types::{DeviceError, Dps, Hsv, LightDevice, ProtocolVersion, SendChannel};
use serde_json::json;
use tuya_sim::{Profile, SimConfig};

/// El color de `test_ambilight.py`: 124°, 0.316, 0.728.
fn verde() -> Hsv {
    Hsv::new(124.0 / 360.0, 0.316, 0.728)
}

#[test]
fn activar_enciende_el_foco_y_lo_pone_en_modo_color() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        // Alguien lo dejó en blanco y apagado.
        rig.sim.set_dps(dps(json!({"20": false, "21": "white"})));
        rig.device.activate().await.unwrap();
        pump(STEP).await;
        assert_eq!(rig.writes(), vec![dps(json!({"20": true, "21": "colour"}))]);
        assert_eq!(rig.sim.dp("21"), Some(json!("colour")));
        assert_eq!(
            rig.device.snapshot().state,
            openlight_types::DeviceSessionState::Active
        );
    });
}

// port: test_ambilight.py::FocoConexion::test_envia_sin_esperar_respuesta_por_el_canal_elegido
#[test]
fn envia_por_el_canal_elegido() {
    run(async {
        let casos = [
            (SendChannel::Colour, json!({"24": "007c013c02d8"})),
            (
                SendChannel::ControlDirect,
                json!({"28": "0007c013c02d800000000"}),
            ),
            (
                SendChannel::ControlGradual,
                json!({"28": "1007c013c02d800000000"}),
            ),
        ];
        for (channel, esperado) in casos {
            let sim = sim(ProtocolVersion::V35).await;
            let caps = caps_of(&sim);
            let rig = Rig::with(sim, |c| c.channel(channel).capabilities(caps));
            rig.start().await;
            rig.activate().await;
            rig.device.send_colour(verde());
            pump(STEP).await;
            assert_eq!(rig.writes(), vec![dps(esperado)], "{channel:?}");
        }
    });
}

// port: test_ambilight.py::FocoConexion::test_nunca_pide_brillo_cero
#[test]
fn nunca_pide_brillo_cero() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.0, 1.0, 0.0));
        pump(STEP).await;
        assert_eq!(rig.writes(), vec![dps(json!({"24": "000003e8000a"}))]);
    });
}

#[test]
fn varios_colores_seguidos_se_juntan_en_el_ultimo() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        // 50 colores antes de que el actor pueda mandar ninguno: solo sale el último.
        for i in 0..50u16 {
            rig.device
                .send_colour(Hsv::new(f32::from(i) / 100.0, 1.0, 1.0));
        }
        pump(STEP).await;
        let esperado = {
            let h = (0.49f32 * 360.0).round() as u32;
            format!("{h:04x}03e803e8")
        };
        assert_eq!(rig.writes(), vec![dps(json!({"24": esperado}))]);
    });
}

#[test]
fn un_color_nuevo_despues_del_envio_vuelve_a_salir() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.activate().await;
        rig.device.send_colour(Hsv::new(0.0, 1.0, 1.0));
        pump(STEP).await;
        rig.device.send_colour(Hsv::new(0.5, 1.0, 1.0));
        pump(STEP).await;
        assert_eq!(
            rig.writes(),
            vec![
                dps(json!({"24": "000003e803e8"})),
                dps(json!({"24": "00b403e803e8"}))
            ]
        );
    });
}

#[test]
fn los_colores_no_valen_hasta_activar() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        rig.start().await;
        rig.sim.reset_stats();
        rig.device.send_colour(verde());
        pump(STEP).await;
        assert!(
            rig.writes().is_empty(),
            "sin activar, el foco no recibe colores"
        );
        // Y el color de antes no se cuela al activar: vale el próximo.
        rig.device.activate().await.unwrap();
        pump(STEP).await;
        assert_eq!(rig.writes(), vec![dps(json!({"20": true, "21": "colour"}))]);
    });
}

#[test]
fn foco_viejo_recibe_colour_data_de_14_caracteres() {
    run(async {
        let sim = tuya_sim::start(
            SimConfig::new(ProtocolVersion::V33)
                .ephemeral()
                .profile(Profile::V1),
        )
        .await
        .unwrap();
        let rig = Rig::with(sim, |c| c);
        let info = rig.start().await;
        assert_eq!(info.capabilities.dp_switch, "1", "deducido de status()");
        rig.activate().await;
        rig.device.send_colour(verde());
        pump(STEP).await;
        assert_eq!(rig.writes(), vec![dps(json!({"5": "7fba83007c51ba"}))]);
    });
}

#[test]
fn el_foco_funciona_por_el_trait_light_device() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V34).await;
        rig.start().await;
        let mut light: Box<dyn LightDevice> = Box::new(rig.device.clone());

        let status = drive(light.status()).await.unwrap();
        assert_eq!(status["21"], json!("colour"));

        drive(light.set_dps(dps(json!({"22": 300})), true))
            .await
            .unwrap();
        assert_eq!(rig.sim.dp("22"), Some(json!(300)));

        // Sin esperar la respuesta, igual llega.
        drive(light.set_dps(dps(json!({"22": 400})), false))
            .await
            .unwrap();
        pump(STEP).await;
        assert_eq!(rig.sim.dp("22"), Some(json!(400)));

        drive(light.heartbeat()).await.unwrap();
        assert!(rig.sim.stats().heartbeats() >= 1);
        assert!(light.health().connected);
        assert!(light.health().bytes_rx > 0);
        assert!(
            light.health().rtt_ms.is_some(),
            "el latido mide la latencia"
        );
    });
}

#[test]
fn antes_de_start_no_hay_conexion() {
    run(async {
        let rig = Rig::new(ProtocolVersion::V35).await;
        let result: Result<Dps, DeviceError> = drive(rig.device.status()).await;
        assert!(
            matches!(result, Err(DeviceError::Unsupported(_))),
            "{result:?}"
        );
        assert_eq!(
            rig.sim.stats().connections,
            0,
            "no se conecta hasta start()"
        );
    });
}
