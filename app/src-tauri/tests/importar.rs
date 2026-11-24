//! Importar el setup del script sobre una copia de la fixture falsa de `openlight-core`.
#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Setup, harness};
use openlight_types::{DeviceId, ErrorCode};
use secrets::{SecretKey, SecretStore};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/openlight-core/tests/fixtures/legacy")
}

/// Copia la fixture a `root/Open RGB`, con el nombre de archivo que lee el importador.
fn legacy_copy(root: &Path) -> PathBuf {
    let legacy = root.join("Open RGB");
    fs::create_dir_all(&legacy).unwrap();
    for entry in fs::read_dir(fixture_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name == "README.md" {
            continue;
        }
        let name = if name == "devices.fake.json" {
            "devices.json".to_owned()
        } else {
            name
        };
        fs::copy(&path, legacy.join(name)).unwrap();
    }
    legacy
}

#[tokio::test]
async fn importar_el_setup_legado_deja_focos_claves_y_calibracion() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let work = tempfile::tempdir().unwrap();
    let legacy = legacy_copy(work.path());
    let before: Vec<_> = fs::read_dir(&legacy)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), fs::read(&p).unwrap())
        })
        .collect();

    let report = h
        .backend
        .import_legacy(legacy.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert!(!report.devices.is_empty());
    assert!(report.calibrations >= 1);
    for warning in &report.warnings {
        assert!(warning.starts_with("import.warning."), "{warning}");
    }

    // Los focos quedan configurados, con todo el monitor como zona y su clave en el llavero
    let devices = h.backend.devices_list().unwrap();
    assert_eq!(
        devices.iter().map(|d| d.id.clone()).collect::<Vec<_>>(),
        report.devices
    );
    assert!(devices.iter().all(|d| d.zones.len() == 1));
    for id in &report.devices {
        assert!(
            h.secrets
                .get(&SecretKey::DeviceLocalKey(id.clone()))
                .unwrap()
                .is_some(),
            "sin clave para {id}"
        );
    }
    assert!(devices.iter().any(|d| d.calibrated));
    assert_eq!(
        h.backend.onboarding_state().unwrap().devices,
        report.devices,
        "el asistente ve los focos importados"
    );

    // Ninguna clave quedó en los JSON de la app, y la carpeta original no se tocó
    for entry in walk(h.backend.data_dir()) {
        let text = fs::read_to_string(&entry).unwrap_or_default();
        assert!(!text.contains("FakeLocalKey"), "clave en {entry:?}");
    }
    for (path, bytes) in before {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{path:?} cambió");
    }

    // Importar de nuevo es seguro y conserva las zonas que el usuario cambió
    let id: DeviceId = report.devices[0].clone();
    h.backend.zones_set(id.clone(), Vec::new()).unwrap();
    h.backend
        .import_legacy(legacy.to_string_lossy().into_owned())
        .await
        .unwrap();
    let again = h.backend.devices_list().unwrap();
    assert!(again.iter().find(|d| d.id == id).unwrap().zones.is_empty());
}

#[tokio::test]
async fn una_carpeta_sin_setup_no_se_importa_y_dice_que_hacer() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let empty = tempfile::tempdir().unwrap();
    let error = h
        .backend
        .import_legacy(empty.path().to_string_lossy().into_owned())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ImportInvalid);
    assert!(!error.actions.is_empty());
    assert!(h.backend.devices_list().unwrap().is_empty());

    let error = h
        .backend
        .import_legacy("/no/existe/de/verdad".into())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ImportInvalid);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
