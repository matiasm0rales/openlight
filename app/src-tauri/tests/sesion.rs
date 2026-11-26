//! La sesión de punta a punta: captura falsa → color → foco simulado → eventos → restaurar.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{
    DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, eventually, harness, sim_id,
};
use openlight_lib::events::AppEvent;
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{
    DeviceId, DeviceSessionState, DisplayId, ErrorCode, Region, SessionState, Zone, ZonePreset,
};
use serde_json::json;

async fn add(h: &Harness, n: usize, zone: Option<ZonePreset>) -> DeviceId {
    let id = sim_id(n);
    h.backend
        .device_add(DeviceAddRequest {
            id: id.clone(),
            name: format!("Foco {n}"),
            ip: Some("127.0.0.1".into()),
        })
        .await
        .unwrap();
    let zones = zone
        .map(|preset| vec![Zone::from_preset(DisplayId(DISPLAY.into()), preset)])
        .unwrap_or_default();
    h.backend.zones_set(id.clone(), zones).unwrap();
    id
}

async fn with_cloud(h: &Harness) {
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
}

fn colour_of(h: &Harness, n: usize) -> String {
    h.sims[n - 1]
        .dp("24")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn hue_of(colour: &str) -> u32 {
    u32::from_str_radix(&colour[..4], 16).unwrap()
}

#[tokio::test]
async fn sin_zonas_no_arranca_y_dice_que_hacer() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    add(&h, 1, None).await;
    let error = h.backend.session_start().await.unwrap_err();
    assert_eq!(error.code, ErrorCode::NoZones);
    assert!(!error.actions.is_empty());
    assert_eq!(h.backend.session_state(), SessionState::Idle);
}

#[tokio::test]
async fn la_sesion_sigue_la_pantalla_emite_eventos_y_al_parar_restaura() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    let id = add(&h, 1, Some(ZonePreset::Full)).await;
    let sim = &h.sims[0];
    let original = sim.dps();
    h.screen.paint([255, 0, 0]);

    let state = h.backend.session_start().await.unwrap();
    assert!(matches!(
        state,
        SessionState::Connecting | SessionState::Active
    ));
    // Arrancar otra vez no duplica la sesión
    h.backend.session_start().await.unwrap();

    // El foco se pone en modo color y recibe rojo (tono 0°)
    eventually("rojo en el foco", Duration::from_secs(8), || {
        sim.dp("21") == Some(json!("colour"))
            && colour_of(&h, 1).len() == 12
            && hue_of(&colour_of(&h, 1)) < 10
            && colour_of(&h, 1) != original["24"].as_str().unwrap()
    })
    .await;
    assert_eq!(h.backend.session_state(), SessionState::Active);

    // Y cuando la pantalla cambia a verde, el foco sigue (el calibrado identidad deja 120°)
    h.screen.paint([0, 255, 0]);
    eventually("verde en el foco", Duration::from_secs(8), || {
        let hue = hue_of(&colour_of(&h, 1));
        (100..=140).contains(&hue)
    })
    .await;

    // Eventos: color en vivo (pantalla vs. enviado), salud con el enlace conectado y estadísticas
    eventually("color en vivo", Duration::from_secs(5), || {
        h.events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::ColorLive(l) if l.device == id && l.sent.is_some()))
    })
    .await;
    eventually("salud conectada", Duration::from_secs(5), || {
        h.events.events().iter().any(|e| {
            matches!(e, AppEvent::DeviceHealth(d)
                if d.device == id && d.state == DeviceSessionState::Active && d.health.connected)
        })
    })
    .await;
    eventually("estadísticas", Duration::from_secs(5), || {
        h.events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::PipelineStats(s) if s.capture_fps > 1.0))
    })
    .await;

    // `color://live` no pasa de 10 Hz por foco
    let before = live_count(&h);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let per_second = live_count(&h) - before;
    assert!(
        per_second <= 11,
        "{per_second} eventos de color en un segundo"
    );

    // El estado global pasó por conectar y activo
    let states: Vec<SessionState> = h.events_where(|e| match e {
        AppEvent::SessionState(s) => Some(s.state),
        _ => None,
    });
    assert_eq!(states.first(), Some(&SessionState::Connecting));
    assert!(states.contains(&SessionState::Active));

    // Parar devuelve el foco a su estado original y lo desconecta
    assert_eq!(h.backend.session_stop().await.unwrap(), SessionState::Idle);
    for (dp, value) in &original {
        assert_eq!(sim.dp(dp).as_ref(), Some(value), "el DP {dp} no volvió");
    }
    eventually("foco desconectado", Duration::from_secs(3), || {
        sim.active_connections() == 0
    })
    .await;
    assert_eq!(h.backend.session_state(), SessionState::Idle);
    let last = h.events_where(|e| match e {
        AppEvent::SessionState(s) => Some(s.state),
        _ => None,
    });
    assert_eq!(last.last(), Some(&SessionState::Idle));
    // Ya no queda "original" pendiente en state.json: la app cerró limpio
    let state = std::fs::read_to_string(h.backend.data_dir().join("state.json")).unwrap();
    assert!(!state.contains("\"original\": {"), "{state}");

    // Parar de nuevo no hace nada
    assert_eq!(h.backend.session_stop().await.unwrap(), SessionState::Idle);
}

fn live_count(h: &Harness) -> usize {
    h.events
        .events()
        .iter()
        .filter(|e| matches!(e, AppEvent::ColorLive(_)))
        .count()
}

#[tokio::test]
async fn dos_focos_siguen_cada_uno_su_mitad_de_la_pantalla() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    with_cloud(&h).await;
    add(&h, 1, Some(ZonePreset::Left)).await;
    add(&h, 2, Some(ZonePreset::Right)).await;
    h.screen.paint([0, 0, 255]);
    h.backend.session_start().await.unwrap();
    eventually("azul en los dos", Duration::from_secs(8), || {
        [1, 2].iter().all(|n| {
            let colour = colour_of(&h, *n);
            colour.len() == 12 && (220..=260).contains(&hue_of(&colour))
        })
    })
    .await;
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn si_el_foco_corta_la_conexion_la_sesion_reconecta_y_sigue() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    add(&h, 1, Some(ZonePreset::Full)).await;
    h.screen.paint([255, 0, 0]);
    h.backend.session_start().await.unwrap();
    let sim = &h.sims[0];
    // El simulado ya arranca en modo color y la conexión se abre antes de estar lista: recién
    // cuando llega el rojo la sesión está conectada. Cortar antes no cuenta como reconexión.
    eventually("conectado", Duration::from_secs(8), || {
        sim.active_connections() == 1 && colour_of(&h, 1) == "000003e803e8"
    })
    .await;

    sim.disconnect_all();
    // Mientras tanto la pantalla cambia: al reconectar tiene que llegar el color nuevo
    h.screen.paint([0, 255, 0]);
    eventually(
        "reconectó con el color nuevo",
        Duration::from_secs(30),
        || sim.active_connections() == 1 && (100..=140).contains(&hue_of(&colour_of(&h, 1))),
    )
    .await;
    eventually(
        "la salud cuenta la reconexión",
        Duration::from_secs(10),
        || {
            h.events.events().iter().any(|e| {
            matches!(e, AppEvent::DeviceHealth(d) if d.health.reconnects >= 1 && d.health.connected)
        })
        },
    )
    .await;
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn con_la_ventana_oculta_no_dibuja_pero_el_foco_sigue_y_al_volver_se_pone_al_dia() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    let id = add(&h, 1, Some(ZonePreset::Full)).await;
    h.screen.paint([255, 0, 0]);
    h.backend.session_start().await.unwrap();
    eventually("rojo en el foco", Duration::from_secs(8), || {
        colour_of(&h, 1).len() == 12 && hue_of(&colour_of(&h, 1)) < 10
    })
    .await;

    h.backend.set_ui_visible(false);
    // Lo que estaba en camino cuando se ocultó termina de salir
    tokio::time::sleep(Duration::from_millis(200)).await;
    let dibujo = |h: &Harness| {
        h.events
            .events()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    AppEvent::ColorLive(_) | AppEvent::DeviceHealth(_) | AppEvent::PipelineStats(_)
                )
            })
            .count()
    };
    let al_ocultar = dibujo(&h);
    h.screen.paint([0, 255, 0]);
    eventually("verde en el foco", Duration::from_secs(8), || {
        (100..=140).contains(&hue_of(&colour_of(&h, 1)))
    })
    .await;
    // Más de un latido de salud y de estadísticas
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        dibujo(&h),
        al_ocultar,
        "oculta no se manda nada para dibujar"
    );

    let antes = h.events.events().len();
    h.backend.set_ui_visible(true);
    let nuevos = h.events.events().split_off(antes);
    assert!(
        nuevos.contains(&AppEvent::SessionState(
            openlight_types::ipc::SessionStateEvent {
                state: SessionState::Active
            }
        )),
        "{nuevos:?}"
    );
    assert!(nuevos.iter().any(|e| matches!(e,
        AppEvent::DeviceHealth(d) if d.device == id && d.health.connected)));
    // El color que manda es el de ahora (verde), no el último que se había visto
    assert!(nuevos.iter().any(|e| matches!(e,
        AppEvent::ColorLive(l) if l.device == id
            && l.screen.h.is_some_and(|h| (0.27..0.40).contains(&h)))));
    h.backend.session_stop().await.unwrap();
}
