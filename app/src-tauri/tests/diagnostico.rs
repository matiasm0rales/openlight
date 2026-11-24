//! Q4 (seguridad): ni el log ni el zip de diagnóstico pueden llevar secretos.
//!
//! Corre el flujo completo contra `tuya-sim` con el log al máximo detalle (`trace`, mucho más
//! que en la app real) y busca en todo lo que salió —el anillo, el archivo de log y cada entrada
//! del zip— la clave local del foco, el Access Secret y un secreto equivocado que se probó.
#![allow(missing_docs)]

mod common;

use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;

use common::{FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Setup, eventually, harness, sim_id};
use openlight_lib::logging::{LogHandle, LogRing, RING_CAPACITY, RingLayer};
use openlight_types::ipc::{CloudCredentialsInput, DeviceAddRequest, LogLevel, TestPattern};
use openlight_types::{Region, Zone, ZonePreset};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const WRONG_SECRET: &str = "secreto-equivocado-0123456789";

#[tokio::test]
async fn ni_el_log_ni_el_zip_llevan_secretos() {
    // Un único subscriber global para esta prueba (cada archivo de `tests/` es su propio proceso):
    // anillo + archivo, como en la app, pero con todo el detalle.
    let tmp = tempfile::tempdir().unwrap();
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let file = std::fs::File::create(log_dir.join("openlight.prueba.log")).unwrap();
    let ring = LogRing::new(RING_CAPACITY);
    tracing_subscriber::registry()
        .with(EnvFilter::new("trace"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(Mutex::new(file))
                .with_ansi(false),
        )
        .with(RingLayer(ring.clone()))
        .try_init()
        .unwrap();

    let h = harness(Setup::default()).await;
    let b = &h.backend;
    b.set_logs(LogHandle {
        ring,
        dir: log_dir.clone(),
        export_dir: tmp.path().join("exports"),
    });
    let sim = &h.sims[0];
    let local_key = String::from_utf8(sim.local_key().to_vec()).unwrap();
    let secrets = [local_key.as_str(), FAKE_ACCESS_SECRET, WRONG_SECRET];

    // Credenciales equivocadas y después las buenas
    let _ = b
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: WRONG_SECRET.into(),
            region: Region::Us,
        })
        .await;
    b.cloud_validate(CloudCredentialsInput {
        access_id: FAKE_ACCESS_ID.into(),
        access_secret: FAKE_ACCESS_SECRET.into(),
        region: Region::Us,
    })
    .await
    .unwrap();

    // Listar, agregar, probar, sesión y renovar la clave
    b.cloud_list_devices().await.unwrap();
    let id = sim_id(1);
    b.device_add(DeviceAddRequest {
        id: id.clone(),
        name: "Escritorio".into(),
        ip: Some("127.0.0.1".into()),
    })
    .await
    .unwrap();
    b.device_test(id.clone(), TestPattern::RedGreenBlue)
        .await
        .unwrap();
    let displays = b.displays_list().unwrap();
    b.zones_set(
        id.clone(),
        vec![Zone::from_preset(displays[0].id.clone(), ZonePreset::Left)],
    )
    .unwrap();
    h.screen.paint([255, 0, 255]);
    b.session_start().await.unwrap();
    eventually("el foco sigue la pantalla", Duration::from_secs(8), || {
        sim.dp("21") == Some(serde_json::json!("colour"))
    })
    .await;
    b.session_stop().await.unwrap();
    b.renew_key(id.clone()).await.unwrap();

    // El log tiene que haber registrado algo del flujo (si no, la prueba no prueba nada)
    let lines = b.diagnostics_log(Some(10_000));
    assert!(lines.len() > 10, "solo {} líneas", lines.len());
    assert!(lines.iter().any(|l| l.target.starts_with("tuya_device")));
    assert!(lines.windows(2).all(|w| w[0].at_ms <= w[1].at_ms));
    assert!(b.diagnostics_log(Some(3)).len() == 3);
    assert!(lines.iter().any(|l| l.level == LogLevel::Debug));
    let ring_text: String = lines
        .iter()
        .map(|l| format!("{} {}\n", l.target, l.message))
        .collect();
    for secret in secrets {
        assert!(!ring_text.contains(secret), "el anillo trae un secreto");
    }

    // El zip: se exporta a una carpeta y se revisa cada entrada
    let out = tmp.path().join("salida");
    let path = b
        .diagnostics_export(Some(out.to_string_lossy().into_owned()))
        .await
        .unwrap();
    assert!(path.starts_with(out.to_str().unwrap()) && path.ends_with(".zip"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    for expected in [
        "health.json",
        "config.json",
        "devices.json",
        "logs/recientes.txt",
        "logs/openlight.prueba.log",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "falta {expected}: {names:?}"
        );
    }
    for name in &names {
        let mut bytes = Vec::new();
        zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for secret in secrets {
            assert!(!text.contains(secret), "{name} trae un secreto");
        }
    }
    // El resumen de salud nombra el foco
    let mut health = String::new();
    zip.by_name("health.json")
        .unwrap()
        .read_to_string(&mut health)
        .unwrap();
    assert!(health.contains(&id.0) && health.contains("app_version"));

    // El archivo de log en disco también está limpio
    let on_disk = std::fs::read_to_string(log_dir.join("openlight.prueba.log")).unwrap();
    for secret in secrets {
        assert!(
            !on_disk.contains(secret),
            "el archivo de log trae un secreto"
        );
    }
}
