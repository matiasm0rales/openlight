//! Monitores, vista previa y zonas con la captura falsa.
#![allow(missing_docs)]

mod common;

use base64::Engine as _;
use common::{DISPLAY, FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Setup, harness, sim_id};
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest};
use openlight_types::{DisplayId, ErrorCode, NormRect, Region, Zone, ZonePreset};

#[tokio::test]
async fn lista_los_monitores_y_da_una_vista_previa_en_png() {
    let h = harness(Setup::default()).await;
    let displays = h.backend.displays_list().unwrap();
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].id.0, DISPLAY);

    h.screen.paint([255, 0, 0]);
    let preview = h
        .backend
        .display_preview(DisplayId(DISPLAY.into()))
        .await
        .unwrap();
    assert_eq!((preview.width, preview.height), (480, 270));
    let png = base64::engine::general_purpose::STANDARD
        .decode(&preview.png_base64)
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    let error = h
        .backend
        .display_preview(DisplayId("no-existe".into()))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NoDisplay);
    assert!(!error.actions.is_empty());
}

#[tokio::test]
async fn las_zonas_se_validan_y_se_guardan_por_foco() {
    let h = harness(Setup::default()).await;
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
    let id = sim_id(1);
    h.backend
        .device_add(DeviceAddRequest {
            id: id.clone(),
            name: "Foco".into(),
            ip: Some("127.0.0.1".into()),
        })
        .await
        .unwrap();

    let left = Zone::from_preset(DisplayId(DISPLAY.into()), ZonePreset::Left);
    h.backend.zones_set(id.clone(), vec![left.clone()]).unwrap();
    assert_eq!(h.backend.devices_list().unwrap()[0].zones, vec![left]);

    let outside = Zone {
        display: DisplayId(DISPLAY.into()),
        rect: NormRect::new(0.8, 0.0, 0.5, 1.0),
        preset: ZonePreset::Custom,
    };
    let error = h.backend.zones_set(id, vec![outside]).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);
    let error = h.backend.zones_set(sim_id(9), Vec::new()).unwrap_err();
    assert_eq!(error.code, ErrorCode::DeviceNotFound);
}
