//! Calibración: mover el foco en vivo y guardar la tabla.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{
    DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, eventually, harness, sim_id,
};
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{DisplayId, ErrorCode, Region};
use serde_json::json;

async fn ready(h: &Harness) {
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
}

fn display() -> DisplayId {
    DisplayId(DISPLAY.into())
}

fn hue(h: &Harness) -> Option<u32> {
    let value = h.sims[0].dp("24")?;
    u32::from_str_radix(&value.as_str()?[..4], 16).ok()
}

#[tokio::test]
async fn calibrar_mueve_el_foco_en_vivo_guarda_la_tabla_y_restaura() {
    let h = harness(Setup::default()).await;
    ready(&h).await;
    let sim = &h.sims[0];
    let original = sim.dps();
    let b = &h.backend;

    let state = b
        .calibration_start(sim_id(1), display(), false)
        .await
        .unwrap();
    assert_eq!(state.points.len(), 12);
    assert_eq!(state.index, 0);
    eventually("rojo al empezar", Duration::from_secs(5), || {
        sim.dp("21") == Some(json!("colour")) && hue(&h) == Some(0)
    })
    .await;

    // Mover el tono del primer punto +20° mueve el foco; más de ±90° se acota
    let state = b.calibration_set(0, Some(20.0), Some(1.2)).await.unwrap();
    assert_eq!(state.points[0].tono_deg, 20.0);
    eventually("tono 20°", Duration::from_secs(5), || hue(&h) == Some(20)).await;
    let state = b.calibration_set(0, Some(500.0), Some(9.0)).await.unwrap();
    assert_eq!(state.points[0].tono_deg, 90.0);
    assert_eq!(state.points[0].sat, 1.5);
    b.calibration_set(0, Some(20.0), Some(1.2)).await.unwrap();

    // Pasar al tono de 120° (índice 4) sin cambiar nada mueve el foco a verde
    let state = b.calibration_set(4, None, None).await.unwrap();
    assert_eq!(state.index, 4);
    eventually("verde", Duration::from_secs(5), || hue(&h) == Some(120)).await;

    // Con la calibración abierta, ni la sesión ni el control manual tocan el foco
    let busy = b
        .device_manual(sim_id(1), openlight_types::ipc::ManualSet::Power(false))
        .await
        .unwrap_err();
    assert_eq!(busy.code, ErrorCode::DeviceBusy);

    b.calibration_save().await.unwrap();
    for (dp, value) in &original {
        assert_eq!(sim.dp(dp).as_ref(), Some(value), "el DP {dp} no volvió");
    }
    assert!(b.devices_list().unwrap()[0].calibrated);

    // La próxima calibración parte de lo guardado
    let state = b
        .calibration_start(sim_id(1), display(), true)
        .await
        .unwrap();
    assert_eq!(state.points.len(), 6);
    assert_eq!(state.points[0].tono_deg, 20.0);
    assert_eq!(state.points[0].sat, 1.2);
    // Cancelar no toca lo guardado y suelta el foco
    b.calibration_set(0, Some(-40.0), None).await.unwrap();
    b.calibration_cancel().await.unwrap();
    let state = b
        .calibration_start(sim_id(1), display(), false)
        .await
        .unwrap();
    assert_eq!(state.points[0].tono_deg, 20.0);
    b.calibration_cancel().await.unwrap();
    assert_eq!(b.calibrating().await, None);
}

#[tokio::test]
async fn el_modo_rapido_conserva_los_demas_tonos_guardados() {
    let h = harness(Setup::default()).await;
    ready(&h).await;
    let b = &h.backend;
    b.calibration_start(sim_id(1), display(), false)
        .await
        .unwrap();
    b.calibration_set(1, Some(40.0), None).await.unwrap(); // 30° → 40°
    b.calibration_set(2, Some(70.0), None).await.unwrap(); // 60° → 70°
    b.calibration_save().await.unwrap();

    b.calibration_start(sim_id(1), display(), true)
        .await
        .unwrap();
    b.calibration_set(1, Some(80.0), None).await.unwrap(); // 60° → 80°
    b.calibration_save().await.unwrap();

    let saved = h
        .backend
        .calibration_start(sim_id(1), display(), false)
        .await
        .unwrap();
    assert_eq!(saved.points[1].tono_deg, 40.0, "el de 30° sigue ahí");
    assert_eq!(saved.points[2].tono_deg, 80.0, "el de 60° se actualizó");
    b.calibration_cancel().await.unwrap();
}

#[tokio::test]
async fn sin_calibracion_en_curso_o_con_un_indice_inexistente_es_un_error() {
    let h = harness(Setup::default()).await;
    ready(&h).await;
    let b = &h.backend;
    assert_eq!(
        b.calibration_set(0, None, None).await.unwrap_err().code,
        ErrorCode::InvalidSetting
    );
    assert_eq!(
        b.calibration_save().await.unwrap_err().code,
        ErrorCode::InvalidSetting
    );
    b.calibration_cancel().await.unwrap(); // cancelar sin nada abierto no es un error
    b.calibration_start(sim_id(1), display(), true)
        .await
        .unwrap();
    assert_eq!(
        b.calibration_set(99, None, None).await.unwrap_err().code,
        ErrorCode::InvalidSetting
    );
    b.calibration_cancel().await.unwrap();
    // Un foco que no existe
    assert_eq!(
        b.calibration_start(sim_id(9), display(), true)
            .await
            .unwrap_err()
            .code,
        ErrorCode::DeviceNotFound
    );
}
