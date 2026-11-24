//! Nube y red local: validar credenciales, listar dispositivos y buscar focos.
#![allow(missing_docs)]

mod common;

use common::{FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Setup, harness, sim_id};
use openlight_lib::events::AppEvent;
use openlight_types::ipc::CloudCredentialsInput;
use openlight_types::{ErrorCode, FixAction, Region};
use secrets::{SecretKey, SecretStore};

fn input(id: &str, secret: &str) -> CloudCredentialsInput {
    CloudCredentialsInput {
        access_id: id.into(),
        access_secret: secret.into(),
        region: Region::Us,
    }
}

#[tokio::test]
async fn credenciales_malas_dan_un_error_con_accion_y_no_se_guardan() {
    let h = harness(Setup::default()).await;
    let error = h
        .backend
        .cloud_validate(input("otro-id", "otro-secreto"))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::CloudInvalidSignature);
    assert!(!error.actions.is_empty());
    assert_eq!(h.secrets.get(&SecretKey::CloudAccessId).unwrap(), None);
    assert!(!h.backend.onboarding_state().unwrap().has_credentials);

    let error = h
        .backend
        .cloud_validate(input("  ", "x"))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);
}

#[tokio::test]
async fn sin_credenciales_la_nube_pide_guardarlas() {
    let h = harness(Setup::default()).await;
    let error = h.backend.cloud_list_devices().await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CloudNoCredentials);
    assert!(!error.actions.is_empty());
}

#[tokio::test]
async fn con_credenciales_buenas_se_guardan_y_se_listan_los_focos_cruzados_con_la_red() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    let account = h
        .backend
        .cloud_validate(input(FAKE_ACCESS_ID, FAKE_ACCESS_SECRET))
        .await
        .unwrap();
    assert_eq!(account.region, Region::Us);
    assert!(h.backend.onboarding_state().unwrap().has_credentials);
    assert_eq!(
        h.secrets
            .get(&SecretKey::CloudAccessSecret)
            .unwrap()
            .unwrap()
            .expose(),
        FAKE_ACCESS_SECRET
    );

    let devices = h.backend.cloud_list_devices().await.unwrap();
    assert_eq!(devices.len(), 2);
    for (i, device) in devices.iter().enumerate() {
        assert_eq!(device.id, sim_id(i + 1));
        assert!(device.is_light);
        assert!(device.capabilities.as_ref().unwrap().supports_colour());
        assert_eq!(device.lan.as_ref().unwrap().ip, "127.0.0.1");
    }
    // El listado avisa de cada foco que encontró en la red
    let found = h.events_where(|e| match e {
        AppEvent::DiscoveryFound(d) => Some(d.id.clone()),
        _ => None,
    });
    assert_eq!(found, vec![sim_id(1), sim_id(2)]);
}

#[tokio::test]
async fn discover_lan_acota_el_tiempo_y_emite_cada_foco() {
    let h = harness(Setup::default()).await;
    let devices = h.backend.discover_lan(f32::NAN).await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, sim_id(1));
    assert!(
        h.events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::DiscoveryFound(_)))
    );
    // No hay acciones colgadas: discover no es un error
    assert!(
        h.events_where(|e| match e {
            AppEvent::ErrorRaised(e) => Some(e.actions.contains(&FixAction::Retry)),
            _ => None,
        })
        .is_empty()
    );
}
