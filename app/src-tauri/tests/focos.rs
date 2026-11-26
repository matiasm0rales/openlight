//! Focos: agregar con clave falsa, probar, control manual, renovar la clave y quitar,
//! contra `tuya-sim`.
#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{
    FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Setup, eventually, harness, harness_with, sim_id,
};
use openlight_types::ipc::{
    CloudCredentialsInput, DeviceAddRequest, ManualSet, SettingScope, TestPattern,
};
use openlight_types::{
    DeviceId, ErrorCode, FixAction, HelpTopic, Hsv, ProtocolVersion, Region, SettingId,
    SettingValue,
};
use secrets::{SecretKey, SecretStore};
use serde_json::json;

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

fn request(id: DeviceId, ip: Option<&str>) -> DeviceAddRequest {
    DeviceAddRequest {
        id,
        name: "Escritorio".into(),
        ip: ip.map(str::to_owned),
    }
}

async fn added(h: &Harness) -> DeviceId {
    with_cloud(h).await;
    let id = sim_id(1);
    h.backend
        .device_add(request(id.clone(), Some("127.0.0.1")))
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn agregar_un_foco_guarda_la_clave_en_el_llavero_y_nunca_en_el_json() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    let id = sim_id(1);
    let device = h
        .backend
        .device_add(request(id.clone(), Some("127.0.0.1")))
        .await
        .unwrap();
    assert_eq!(device.name, "Escritorio");
    assert_eq!(device.ip.as_deref(), Some("127.0.0.1"));
    assert!(!device.auto_ip, "IP fija: nunca se busca otra");
    assert_eq!(
        device.version,
        ProtocolVersion::V35,
        "la versión sale de la red"
    );
    assert!(device.capabilities.supports_colour());
    assert_eq!(
        device.zones.len(),
        1,
        "sigue todo el monitor, como el script"
    );
    assert!(!device.calibrated);

    let key = h
        .secrets
        .get(&SecretKey::DeviceLocalKey(id.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(key.expose(), "OpenLightSimKey1");
    // La clave no aparece en ningún archivo de la carpeta de datos
    for entry in walk(h.backend.data_dir()) {
        let text = std::fs::read_to_string(&entry).unwrap_or_default();
        assert!(!text.contains("OpenLightSimKey1"), "clave en {entry:?}");
        assert!(!text.contains(FAKE_ACCESS_SECRET), "secreto en {entry:?}");
    }
    assert_eq!(h.backend.devices_list().unwrap(), vec![device]);
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[tokio::test]
async fn sin_ip_fija_la_ip_se_busca_sola_y_un_id_ajeno_no_se_agrega() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    let device = h
        .backend
        .device_add(request(sim_id(1), None))
        .await
        .unwrap();
    assert!(device.auto_ip);
    assert_eq!(
        device.ip.as_deref(),
        Some("127.0.0.1"),
        "última IP conocida de la red"
    );

    let error = h
        .backend
        .device_add(request(DeviceId("de-otra-cuenta".into()), None))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::DeviceNotFound);
    let error = h
        .backend
        .device_add(request(sim_id(1), Some("no-es-una-ip")))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidSetting);
}

#[tokio::test]
async fn actualizar_cambia_nombre_e_ip() {
    let h = harness(Setup::default()).await;
    let id = added(&h).await;
    let updated = h
        .backend
        .device_update(DeviceAddRequest {
            id: id.clone(),
            name: "Sala".into(),
            ip: None,
        })
        .unwrap();
    assert_eq!(updated.name, "Sala");
    assert!(updated.auto_ip);
    let updated = h
        .backend
        .device_update(DeviceAddRequest {
            id,
            name: String::new(),
            ip: Some("192.0.2.9".into()),
        })
        .unwrap();
    assert_eq!(updated.name, "Sala", "un nombre vacío no lo borra");
    assert_eq!(updated.ip.as_deref(), Some("192.0.2.9"));
    assert!(!updated.auto_ip);
}

#[tokio::test]
async fn device_test_corre_rojo_verde_azul_y_devuelve_el_foco_a_su_estado() {
    let h = harness(Setup::default()).await;
    let id = added(&h).await;
    let sim = &h.sims[0];
    let original = sim.dps();

    h.backend
        .device_test(id.clone(), TestPattern::RedGreenBlue)
        .await
        .unwrap();

    // Escribió los tres colores (h = 0°, 120° y 240° al máximo) y después restauró
    let colours: Vec<String> = sim
        .writes()
        .iter()
        .filter_map(|w| w.get("24").and_then(|v| v.as_str()).map(str::to_owned))
        .collect();
    for wanted in ["000003e803e8", "007803e803e8", "00f003e803e8"] {
        assert!(
            colours.iter().any(|c| c == wanted),
            "falta {wanted} en {colours:?}"
        );
    }
    for (dp, value) in &original {
        assert_eq!(sim.dp(dp).as_ref(), Some(value), "el DP {dp} no volvió");
    }
}

#[tokio::test]
async fn el_control_manual_enciende_pone_color_y_blanco_y_suelta_la_conexion_sola() {
    let h = harness(Setup::default()).await;
    let id = added(&h).await;
    let sim = &h.sims[0];
    let b = &h.backend;

    b.device_manual(id.clone(), ManualSet::Colour(Hsv::new(0.0, 1.0, 1.0)))
        .await
        .unwrap();
    eventually("color escrito", Duration::from_secs(3), || {
        sim.dp("21") == Some(json!("colour")) && sim.dp("24") == Some(json!("000003e803e8"))
    })
    .await;

    b.device_manual(
        id.clone(),
        ManualSet::White {
            brightness: 1.0,
            temperature: 0.0,
        },
    )
    .await
    .unwrap();
    eventually("blanco escrito", Duration::from_secs(3), || {
        sim.dp("21") == Some(json!("white")) && sim.dp("22") == Some(json!(1000))
    })
    .await;

    b.device_manual(id.clone(), ManualSet::Power(false))
        .await
        .unwrap();
    eventually("apagado", Duration::from_secs(3), || {
        sim.dp("20") == Some(json!(false))
    })
    .await;

    // Sin usarse, la conexión se suelta (manual_idle = 400 ms en el arnés)
    eventually("conexión suelta", Duration::from_secs(5), || {
        sim.active_connections() == 0
    })
    .await;
    // y se vuelve a abrir sola al usarlo de nuevo
    b.device_manual(id, ManualSet::Power(true)).await.unwrap();
    eventually("encendido", Duration::from_secs(3), || {
        sim.dp("20") == Some(json!(true))
    })
    .await;
}

#[tokio::test]
async fn la_conexion_manual_se_cierra_apenas_el_efecto_toma_el_foco() {
    // Como un foco real, que atiende una sola conexión: si la del control manual quedara
    // abierta hasta vencer su plazo, el efecto no podría conectarse
    let h = harness_with(Setup::default(), |o| {
        o.manual_idle = Duration::from_secs(60)
    })
    .await;
    let id = added(&h).await;
    let sim = &h.sims[0];
    sim.update_faults(|f| f.exclusive = true);
    h.backend
        .device_manual(id.clone(), ManualSet::Power(true))
        .await
        .unwrap();
    assert_eq!(sim.active_connections(), 1);

    h.screen.paint([255, 0, 0]);
    h.backend.session_start().await.unwrap();
    eventually("el efecto toma el foco", Duration::from_secs(8), || {
        sim.dp("24") == Some(json!("000003e803e8"))
    })
    .await;
    h.backend.session_stop().await.unwrap();
}

#[tokio::test]
async fn un_foco_que_no_esta_en_la_red_falla_con_error_y_accion() {
    let h = harness(Setup::default()).await;
    let id = added(&h).await;
    // Se cae el foco simulado y el backend espera poco
    h.sims[0].update_faults(|f| f.mute = true);
    let started = std::time::Instant::now();
    let error = h
        .backend
        .device_manual(id, ManualSet::Power(true))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error.code,
            ErrorCode::DeviceNotFound | ErrorCode::DeviceTimeout | ErrorCode::DeviceKeyRejected
        ),
        "{:?}",
        error.code
    );
    assert!(!error.actions.is_empty());
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[tokio::test]
async fn renovar_la_clave_baja_la_nueva_y_quitar_borra_todo() {
    let h = harness(Setup::default()).await;
    let id = added(&h).await;
    // La clave del llavero se pierde (clave vieja): renovarla la repone desde la nube
    h.secrets
        .set(
            &SecretKey::DeviceLocalKey(id.clone()),
            &openlight_types::Secret::new("clave-vieja-0000".into()),
        )
        .unwrap();
    h.backend.renew_key(id.clone()).await.unwrap();
    assert_eq!(
        h.secrets
            .get(&SecretKey::DeviceLocalKey(id.clone()))
            .unwrap()
            .unwrap()
            .expose(),
        "OpenLightSimKey1"
    );

    h.backend
        .settings_set(
            SettingScope::Device(id.clone()),
            SettingId::Saturation,
            SettingValue::Number(1.5),
        )
        .unwrap();
    h.backend.device_remove(id.clone()).await.unwrap();
    assert!(h.backend.devices_list().unwrap().is_empty());
    assert_eq!(
        h.secrets
            .get(&SecretKey::DeviceLocalKey(id.clone()))
            .unwrap(),
        None
    );
    let error = h.backend.device_remove(id).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::DeviceNotFound);
}

// ---------- versión e IP al agregar: nunca se adivinan ----------

use common::ScanMode;
use openlight_types::ipc::DiscoveredDevice;

fn lan(version: ProtocolVersion, ip: &str) -> ScanMode {
    ScanMode::Found(vec![DiscoveredDevice {
        id: sim_id(1),
        ip: ip.into(),
        version,
        product_key: None,
    }])
}

#[tokio::test]
async fn si_la_busqueda_no_puede_escuchar_la_red_no_se_guarda_un_foco_a_ciegas() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    h.scan.set(ScanMode::Fails);
    let error = h
        .backend
        .device_add(request(sim_id(1), Some("127.0.0.1")))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::LanScanFailed);
    // Buscar de nuevo primero, y la ayuda para cerrar otros programas
    assert_eq!(error.actions[0], FixAction::Retry);
    assert!(
        error
            .actions
            .contains(&FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting))
    );
    assert_eq!(error.params["detail"], "puerto UDP en uso");
    assert!(
        h.backend.devices_list().unwrap().is_empty(),
        "no quedó nada guardado"
    );
    assert_eq!(
        h.secrets
            .get(&SecretKey::DeviceLocalKey(sim_id(1)))
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn si_el_foco_no_aparece_en_la_red_es_un_error_y_no_se_asume_3_3() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    h.scan.set(ScanMode::Empty);
    for ip in [None, Some("127.0.0.1")] {
        let error = h
            .backend
            .device_add(request(sim_id(1), ip))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::DeviceNotFoundOnLan);
        assert!(error.actions.contains(&FixAction::Retry));
    }
    assert!(h.backend.devices_list().unwrap().is_empty());
}

#[tokio::test]
async fn volver_a_agregar_conserva_la_version_y_la_ip_aunque_la_red_no_responda() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    // La primera vez el foco anuncia 3.4 en otra IP
    h.scan.set(lan(ProtocolVersion::V34, "192.0.2.50"));
    let first = h
        .backend
        .device_add(request(sim_id(1), None))
        .await
        .unwrap();
    assert_eq!(first.version, ProtocolVersion::V34);
    assert_eq!(first.ip.as_deref(), Some("192.0.2.50"));

    // Re-agregar con la búsqueda rota, y después con la red vacía: queda lo de antes
    for mode in [ScanMode::Fails, ScanMode::Empty] {
        h.scan.set(mode);
        // otra instancia de la app: sin caché de la red
        let again = common::reopen_with(&h)
            .device_add(request(sim_id(1), None))
            .await;
        let again = again.unwrap();
        assert_eq!(again.version, ProtocolVersion::V34, "no vuelve a 3.3");
        assert_eq!(again.ip.as_deref(), Some("192.0.2.50"));
    }

    // Y si ahora sí aparece, manda lo que anuncia (el foco pudo cambiar de IP)
    h.scan.set(lan(ProtocolVersion::V35, "192.0.2.51"));
    let moved = common::reopen_with(&h)
        .device_add(request(sim_id(1), None))
        .await
        .unwrap();
    assert_eq!(moved.version, ProtocolVersion::V35);
    assert_eq!(moved.ip.as_deref(), Some("192.0.2.51"));
}

#[tokio::test]
async fn actualizar_nombre_o_ip_no_toca_la_version() {
    let h = harness(Setup::default()).await;
    with_cloud(&h).await;
    h.scan.set(lan(ProtocolVersion::V35, "127.0.0.1"));
    let id = sim_id(1);
    h.backend
        .device_add(request(id.clone(), None))
        .await
        .unwrap();
    for ip in [Some("192.0.2.9".to_owned()), None] {
        let updated = h
            .backend
            .device_update(DeviceAddRequest {
                id: id.clone(),
                name: "Otro nombre".into(),
                ip,
            })
            .unwrap();
        assert_eq!(updated.version, ProtocolVersion::V35);
    }
}
