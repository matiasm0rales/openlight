//! Criterio de aceptación de I1a: el flujo completo contra `tuya-sim` con una captura falsa.
//!
//! credenciales → listar → agregar un foco con clave falsa → probarlo → zonas → sesión con
//! eventos de color y salud → parar (restaura) → importar el setup del script.
#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::{FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Setup, eventually, harness, sim_id};
use openlight_lib::events::AppEvent;
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest, OnboardingStep, TestPattern};
use openlight_types::{DeviceSessionState, Region, SessionState, Zone, ZonePreset};

#[tokio::test]
async fn el_flujo_completo_del_asistente_a_la_sesion() {
    let h = harness(Setup::default()).await;
    let b = &h.backend;
    let sim = &h.sims[0];
    let original = sim.dps();

    // 1-6. Asistente: arranca de cero y se guardan las credenciales
    assert!(!b.onboarding_state().unwrap().has_credentials);
    b.cloud_validate(CloudCredentialsInput {
        access_id: FAKE_ACCESS_ID.into(),
        access_secret: FAKE_ACCESS_SECRET.into(),
        region: Region::Us,
    })
    .await
    .unwrap();
    b.onboarding_save(
        OnboardingStep::ChooseDevices,
        vec![OnboardingStep::Credentials],
    )
    .unwrap();

    // 7. Elegir el foco: la nube lo lista, cruzado con la red y con sus capacidades
    let listed = b.cloud_list_devices().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].lan.is_some() && listed[0].capabilities.is_some());
    let id = sim_id(1);
    b.device_add(DeviceAddRequest {
        id: id.clone(),
        name: "Escritorio".into(),
        ip: Some("127.0.0.1".into()),
    })
    .await
    .unwrap();

    // 8. Probarlo: rojo, verde y azul, y el foco vuelve a como estaba
    b.device_test(id.clone(), TestPattern::RedGreenBlue)
        .await
        .unwrap();
    for (dp, value) in &original {
        assert_eq!(sim.dp(dp).as_ref(), Some(value), "tras la prueba, DP {dp}");
    }

    // 9. Zonas: el foco sigue la mitad izquierda del monitor
    let displays = b.displays_list().unwrap();
    let zone = Zone::from_preset(displays[0].id.clone(), ZonePreset::Left);
    b.zones_set(id.clone(), vec![zone]).unwrap();

    // 11. Arrancar: color y salud llegan por eventos
    h.screen.paint([255, 0, 255]);
    b.session_start().await.unwrap();
    eventually("el foco sigue la pantalla", Duration::from_secs(8), || {
        sim.dp("21") == Some(serde_json::json!("colour"))
            && sim
                .dp("24")
                .and_then(|v| {
                    v.as_str()
                        .map(|s| u32::from_str_radix(&s[..4], 16).unwrap())
                })
                .is_some_and(|hue| (280..=320).contains(&hue))
    })
    .await;
    eventually("eventos de color y salud", Duration::from_secs(5), || {
        let events = h.events.events();
        events
            .iter()
            .any(|e| matches!(e, AppEvent::ColorLive(l) if l.device == id))
            && events.iter().any(|e| {
                matches!(e, AppEvent::DeviceHealth(d)
                    if d.device == id && d.state == DeviceSessionState::Active)
            })
    })
    .await;
    assert_eq!(b.session_state(), SessionState::Active);

    // Parar restaura el foco
    b.session_stop().await.unwrap();
    for (dp, value) in &original {
        assert_eq!(sim.dp(dp).as_ref(), Some(value), "tras parar, DP {dp}");
    }
    assert_eq!(b.session_state(), SessionState::Idle);

    // Importar el setup del script (copia de la fixture falsa de openlight-core)
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/openlight-core/tests/fixtures/legacy");
    let legacy = h.dir.path().join("Open RGB");
    fs::create_dir_all(&legacy).unwrap();
    for entry in fs::read_dir(fixtures).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name != "README.md" {
            let name = name.replace("devices.fake.json", "devices.json");
            fs::copy(&path, legacy.join(name)).unwrap();
        }
    }
    let report = b
        .import_legacy(legacy.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert!(!report.devices.is_empty());
    assert!(
        b.devices_list().unwrap().len() > 1,
        "el foco agregado y los importados conviven"
    );
}
