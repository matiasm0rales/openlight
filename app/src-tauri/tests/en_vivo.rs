//! Cambios con el efecto andando: ajustes, perfiles, zonas y focos que entran o salen, con focos
//! simulados y captura falsa.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{
    DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, eventually, harness, harness_with,
    sim_id,
};
use openlight_types::ipc::{
    CloudCredentialsInput, DeviceAddRequest, ManualSet, SettingScope, TestPattern,
};
use openlight_types::{
    DeviceId, DisplayId, Dps, ErrorCode, FixAction, ProfileId, Region, SessionState, SettingId,
    SettingValue, Zone, ZonePreset,
};
use serde_json::json;
use tuya_sim::SimHandle;

const SOON: Duration = Duration::from_secs(8);
const BLANCO: [u8; 3] = [255, 255, 255];

fn num(v: f64) -> SettingValue {
    SettingValue::Number(v)
}

fn full() -> Vec<Zone> {
    vec![Zone::from_preset(
        DisplayId(DISPLAY.into()),
        ZonePreset::Full,
    )]
}

/// Agrega los focos simulados, todos mirando el monitor entero.
async fn setup(h: &Harness) -> Vec<DeviceId> {
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
    let mut ids = Vec::new();
    for n in 1..=h.sims.len() {
        h.backend
            .device_add(DeviceAddRequest {
                id: sim_id(n),
                name: format!("Foco {n}"),
                ip: Some("127.0.0.1".into()),
            })
            .await
            .unwrap();
        h.backend.zones_set(sim_id(n), full()).unwrap();
        ids.push(sim_id(n));
    }
    ids
}

/// El último color que recibió el foco: `(tono en grados, brillo de 0 a 1000)`.
fn colour(sim: &SimHandle) -> (u32, u32) {
    let text = sim
        .dp("24")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    if text.len() != 12 {
        return (u32::MAX, u32::MAX);
    }
    let part = |range| u32::from_str_radix(&text[range], 16).unwrap();
    (part(0..4), part(8..12))
}

fn brightness_is(sim: &SimHandle, wanted: u32) -> bool {
    sim.dp("21") == Some(json!("colour")) && colour(sim).1.abs_diff(wanted) <= 15
}

fn hue_is(sim: &SimHandle, wanted: u32) -> bool {
    sim.dp("21") == Some(json!("colour")) && colour(sim).0.abs_diff(wanted) <= 15
}

/// Arranca con la pantalla en blanco y espera a que todos lleguen al brillo pleno.
async fn running(h: &Harness) {
    h.screen.paint(BLANCO);
    h.backend.session_start().await.unwrap();
    eventually("todos al máximo", SOON, || {
        h.sims.iter().all(|s| brightness_is(s, 1000))
    })
    .await;
}

fn same(sim: &SimHandle, original: &Dps) -> bool {
    original
        .iter()
        .all(|(dp, v)| sim.dp(dp).as_ref() == Some(v))
}

#[tokio::test]
async fn bajar_el_brillo_maximo_cambia_lo_enviado_sin_reiniciar() {
    let h = harness(Setup::default()).await;
    setup(&h).await;
    running(&h).await;

    h.backend
        .settings_set(SettingScope::Global, SettingId::BrightnessMax, num(0.3))
        .unwrap();
    eventually("brillo al 30 %", SOON, || brightness_is(&h.sims[0], 300)).await;
    h.backend
        .settings_reset(SettingScope::Global, Some(SettingId::BrightnessMax))
        .unwrap();
    eventually("brillo pleno de nuevo", SOON, || {
        brightness_is(&h.sims[0], 1000)
    })
    .await;

    assert_eq!(h.sims[0].stats().connections, 1, "no reconectó");
    assert_eq!(h.screen.capture_starts().len(), 1, "ni rearmó la captura");
    assert_eq!(h.backend.session_state(), SessionState::Active);
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn cambiar_el_brillo_minimo_y_el_realce_tambien_vale_al_instante() {
    let h = harness(Setup::default()).await;
    let ids = setup(&h).await;
    h.screen.paint([0, 0, 0]);
    h.backend.session_start().await.unwrap();
    eventually("pantalla negra: brillo mínimo", SOON, || {
        brightness_is(&h.sims[0], 50)
    })
    .await;
    // El brillo propio del foco, como el control del panel
    h.backend
        .settings_set(
            SettingScope::Device(ids[0].clone()),
            SettingId::BrightnessMin,
            num(0.4),
        )
        .unwrap();
    eventually("mínimo nuevo", SOON, || brightness_is(&h.sims[0], 400)).await;

    // Un rojo apagado: el realce sube la saturación que se manda
    h.screen.paint([200, 150, 150]);
    let saturation = || {
        let text = h.sims[0].dp("24").unwrap().as_str().unwrap().to_owned();
        u32::from_str_radix(&text[4..8], 16).unwrap()
    };
    eventually("saturación con el realce de fábrica", SOON, || {
        saturation().abs_diff(325) <= 20
    })
    .await;
    h.backend
        .settings_set(SettingScope::Global, SettingId::Saturation, num(2.5))
        .unwrap();
    eventually("saturación realzada", SOON, || {
        saturation().abs_diff(625) <= 20
    })
    .await;
    assert_eq!(h.sims[0].stats().connections, 1);
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn cambiar_los_cuadros_por_segundo_rearma_la_captura_y_sigue_mandando() {
    let h = harness(Setup::default()).await;
    setup(&h).await;
    running(&h).await;

    h.backend
        .settings_set(SettingScope::Global, SettingId::CaptureFps, num(30.0))
        .unwrap();
    eventually("captura a 30", SOON, || {
        h.screen.capture_starts() == [20.0, 30.0]
    })
    .await;
    h.screen.paint([0, 255, 0]);
    eventually("verde", SOON, || hue_is(&h.sims[0], 120)).await;
    assert_eq!(h.sims[0].stats().connections, 1, "el foco no reconectó");
    assert_eq!(h.backend.session_state(), SessionState::Active);
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn activar_editar_y_borrar_un_perfil_vale_en_marcha() {
    let h = harness(Setup::default()).await;
    setup(&h).await;
    running(&h).await;

    h.backend
        .profiles_activate(Some(ProfileId("work".into())))
        .unwrap();
    eventually("Trabajo: 50 %", SOON, || brightness_is(&h.sims[0], 500)).await;
    let mine = h
        .backend
        .profiles_create("Tenue".into(), Some(ProfileId("work".into())))
        .unwrap();
    h.backend.profiles_activate(Some(mine.id.clone())).unwrap();
    let mut edited = mine.clone();
    edited.values.0.insert(SettingId::BrightnessMax, num(0.2));
    h.backend.profiles_update(edited).unwrap();
    eventually("perfil editado: 20 %", SOON, || {
        brightness_is(&h.sims[0], 200)
    })
    .await;
    h.backend.profiles_delete(mine.id).unwrap();
    eventually("sin perfil: 100 %", SOON, || {
        brightness_is(&h.sims[0], 1000)
    })
    .await;
    assert_eq!(h.sims[0].stats().connections, 1);
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn un_foco_sin_zonas_sale_del_efecto_y_al_volver_a_tenerlas_entra() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    let ids = setup(&h).await;
    let original = h.sims[1].dps();
    running(&h).await;

    h.backend.zones_set(ids[1].clone(), Vec::new()).unwrap();
    eventually("el foco 2 volvió a su estado", SOON, || {
        same(&h.sims[1], &original) && h.sims[1].active_connections() == 0
    })
    .await;
    h.screen.paint([0, 0, 255]);
    eventually("el foco 1 sigue", SOON, || hue_is(&h.sims[0], 240)).await;

    h.backend.zones_set(ids[1].clone(), full()).unwrap();
    eventually("el foco 2 entra", SOON, || hue_is(&h.sims[1], 240)).await;
    assert_eq!(h.sims[0].stats().connections, 1);

    // Sin ninguno con zonas, el efecto se detiene solo
    h.backend.zones_set(ids[0].clone(), Vec::new()).unwrap();
    h.backend.zones_set(ids[1].clone(), Vec::new()).unwrap();
    eventually("efecto detenido", SOON, || {
        h.backend.session_state() == SessionState::Idle
    })
    .await;
}

#[tokio::test]
async fn un_foco_excluido_no_recibe_colores_y_el_otro_sigue_sin_cortarse() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    let ids = setup(&h).await;
    let original = h.sims[1].dps();
    running(&h).await;

    h.backend
        .set_device_in_effect(ids[1].clone(), false)
        .await
        .unwrap();
    // Al volver del comando el foco ya está como antes y libre
    assert!(same(&h.sims[1], &original), "{:?}", h.sims[1].dps());
    eventually("foco 2 desconectado", Duration::from_secs(3), || {
        h.sims[1].active_connections() == 0
    })
    .await;
    h.sims[1].reset_stats();
    h.screen.paint([0, 0, 255]);
    eventually("el foco 1 sigue", SOON, || hue_is(&h.sims[0], 240)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        h.sims[1].stats().commands(),
        0,
        "al excluido no le llega nada"
    );
    assert!(same(&h.sims[1], &original));
    assert_eq!(h.sims[0].stats().connections, 1, "el otro no se cortó");
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn el_control_manual_toma_un_foco_sacado_y_despues_vuelve_al_efecto() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    let ids = setup(&h).await;
    running(&h).await;
    h.backend
        .set_device_in_effect(ids[1].clone(), false)
        .await
        .unwrap();

    h.backend
        .device_manual(
            ids[1].clone(),
            ManualSet::White {
                brightness: 1.0,
                temperature: 0.5,
            },
        )
        .await
        .unwrap();
    eventually("blanco", SOON, || {
        h.sims[1].dp("21") == Some(json!("white")) && h.sims[1].dp("22") == Some(json!(1000))
    })
    .await;
    h.backend
        .device_manual(ids[1].clone(), ManualSet::Power(false))
        .await
        .unwrap();
    eventually("apagado", SOON, || h.sims[1].dp("20") == Some(json!(false))).await;

    h.backend
        .set_device_in_effect(ids[1].clone(), true)
        .await
        .unwrap();
    eventually("de vuelta en el efecto", SOON, || {
        h.sims[1].dp("20") == Some(json!(true)) && brightness_is(&h.sims[1], 1000)
    })
    .await;
    h.backend.session_stop().await.unwrap();
    // Al detener vuelve a como lo dejó la persona: apagado y en blanco
    assert_eq!(h.sims[1].dp("20"), Some(json!(false)));
    assert_eq!(h.sims[1].dp("21"), Some(json!("white")));
}

#[tokio::test]
async fn con_el_foco_en_el_efecto_se_ofrece_sacarlo_y_no_se_culpa_a_otra_app() {
    let h = harness(Setup::default()).await;
    let ids = setup(&h).await;
    running(&h).await;
    let id = ids[0].clone();

    let manual = h
        .backend
        .device_manual(id.clone(), ManualSet::Power(false))
        .await
        .unwrap_err();
    let calibration = h
        .backend
        .calibration_start(id.clone(), DisplayId(DISPLAY.into()), true)
        .await
        .unwrap_err();
    let test = h
        .backend
        .device_test(id.clone(), TestPattern::Blink)
        .await
        .unwrap_err();
    let remove = h.backend.device_remove(id.clone()).await.unwrap_err();
    for error in [manual, calibration, test, remove] {
        assert_eq!(error.code, ErrorCode::DeviceInEffect);
        assert_eq!(error.actions, vec![FixAction::RemoveFromEffect]);
        assert_eq!(error.params["device"], id.0);
    }
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn si_el_foco_rechaza_la_conexion_el_error_sigue_siendo_de_foco_ocupado() {
    let h = harness(Setup::default()).await;
    let ids = setup(&h).await;
    // Otra app lo tiene: el foco corta la conexión apenas llega
    h.sims[0].update_faults(|f| f.occupied = true);
    let error = h
        .backend
        .device_manual(ids[0].clone(), ManualSet::Power(true))
        .await
        .unwrap_err();
    assert!(
        !matches!(
            error.code,
            ErrorCode::DeviceInEffect | ErrorCode::DeviceInUse
        ),
        "{:?}",
        error.code
    );
}

#[tokio::test]
async fn un_foco_ocupado_al_ganar_zonas_entra_apenas_se_libera() {
    // La prueba de colores dura casi dos segundos: las zonas llegan con el foco tomado
    let h = harness_with(Setup { sims: 2, lan: true }, |o| {
        o.test_step = Duration::from_millis(600)
    })
    .await;
    let ids = setup(&h).await;
    h.backend.zones_set(ids[1].clone(), Vec::new()).unwrap();
    h.screen.paint(BLANCO);
    h.backend.session_start().await.unwrap();
    eventually("foco 1 en el efecto", SOON, || {
        brightness_is(&h.sims[0], 1000)
    })
    .await;

    let backend = h.backend.clone();
    let busy = ids[1].clone();
    let test =
        tokio::spawn(async move { backend.device_test(busy, TestPattern::RedGreenBlue).await });
    eventually("la prueba tiene el foco", SOON, || {
        h.sims[1].active_connections() == 1
    })
    .await;
    h.backend.zones_set(ids[1].clone(), full()).unwrap();
    test.await.unwrap().unwrap();

    // Sin ningún otro cambio de por medio: el blanco de la pantalla (sin tono ni saturación)
    eventually("el foco 2 entra", SOON, || {
        h.sims[1].dp("24") == Some(json!("0000000003e8"))
    })
    .await;
    h.backend.session_stop().await.unwrap();
}
