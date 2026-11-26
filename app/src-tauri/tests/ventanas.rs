//! La ventana principal se destruye al cerrarla y se vuelve a crear al abrirla. Usa la
//! configuración real sobre el runtime falso de Tauri; que cerrar libere el proceso de la página
//! se comprueba con `examples/banco.rs`.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, harness, sim_id};
use openlight_lib::events::AppEvent;
use openlight_lib::windows::{self, CALIBRATION, MAIN};
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{DisplayId, Region, SessionState, Zone, ZonePreset};
use secrets::{SecretKey, SecretStore};
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Manager, WebviewWindowBuilder};

/// La app con la configuración real y el backend de las pruebas.
fn app(h: &Harness) -> tauri::App<MockRuntime> {
    let builder = mock_builder();
    #[cfg(target_os = "macos")]
    let builder = builder.enable_macos_default_menu(false);
    let app = builder
        .build(tauri::generate_context!(test = true))
        .unwrap();
    app.manage(h.backend.clone());
    app
}

async fn with_device() -> Harness {
    let h = harness(Setup::default()).await;
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
    h.backend
        .device_add(DeviceAddRequest {
            id: sim_id(1),
            name: "Foco".into(),
            ip: Some("127.0.0.1".into()),
        })
        .await
        .unwrap();
    h
}

/// Los errores que llegaron a la UI.
fn raised(h: &Harness) -> usize {
    h.events_where(|e| matches!(e, AppEvent::ErrorRaised(_)).then_some(()))
        .len()
}

#[tokio::test]
async fn la_ventana_principal_no_se_crea_sola_y_abrirla_la_crea_una_vez() {
    let h = harness(Setup::default()).await;
    let app = app(&h);
    assert!(
        app.get_webview_window(MAIN).is_none(),
        "tauri.conf.json: la principal va con `create: false`"
    );
    windows::open_main(app.handle()).unwrap();
    assert!(app.get_webview_window(MAIN).is_some());
    windows::open_main(app.handle()).unwrap();
    assert_eq!(app.webview_windows().len(), 1);
}

#[tokio::test]
async fn al_destruirse_la_ventana_los_errores_esperan_a_la_proxima() {
    let h = with_device().await;
    let app = app(&h);
    h.backend.ui_ready();
    h.backend.ui_hold(true);
    windows::destroyed(app.handle(), MAIN);
    assert!(!h.backend.keeps_window(), "la ventana nueva no hereda nada");

    // Un foco con zonas pero sin clave se avisa con un evento, y todavía no hay ventana
    h.backend
        .zones_set(
            sim_id(1),
            vec![Zone::from_preset(
                DisplayId(DISPLAY.into()),
                ZonePreset::Full,
            )],
        )
        .unwrap();
    h.secrets
        .delete(&SecretKey::DeviceLocalKey(sim_id(1)))
        .unwrap();
    let _ = h.backend.session_start().await;
    assert_eq!(h.backend.session_state(), SessionState::Idle);
    let before = raised(&h);
    assert!(before > 0);
    h.backend.ui_ready();
    assert_eq!(raised(&h), before * 2, "los recibe la ventana nueva");
}

#[tokio::test]
async fn sin_ninguna_ventana_la_calibracion_se_cancela() {
    let h = with_device().await;
    let app = app(&h);
    windows::open_main(app.handle()).unwrap();
    WebviewWindowBuilder::new(&app, CALIBRATION, Default::default())
        .build()
        .unwrap();
    h.backend
        .calibration_start(sim_id(1), DisplayId(DISPLAY.into()), true)
        .await
        .unwrap();

    // Queda la de calibración: sigue
    windows::destroyed(app.handle(), MAIN);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.backend.calibrating().await, Some(sim_id(1)));

    // La única que quedaba: se cancela y el foco vuelve a su estado
    let h2 = with_device().await;
    let solo = self::app(&h2);
    h2.backend
        .calibration_start(sim_id(1), DisplayId(DISPLAY.into()), true)
        .await
        .unwrap();
    windows::destroyed(solo.handle(), CALIBRATION);
    common::eventually("sin calibración", Duration::from_secs(8), || {
        h2.sims[0].active_connections() == 0
    })
    .await;
    assert_eq!(h2.backend.calibrating().await, None);
}
