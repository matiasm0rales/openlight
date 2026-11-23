//! Importar el setup del script Python, siempre sobre una COPIA de la carpeta.
//!
//! La fixture (`tests/fixtures/legacy`) tiene la forma de los archivos reales con valores
//! inventados. `devices.json` está en el `.gitignore`, así que la fixture se llama
//! `devices.fake.json` y se copia con el nombre que lee el importador.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use color_engine::Calibration;
use openlight_core::import::{
    WARN_CALIBRATION_UNASSIGNED, WARN_NO_KEY, WARN_SKIPPED_NON_LIGHT, WARN_VERSION_ASSUMED,
};
use openlight_core::{CoreError, Store, import_legacy};
use openlight_types::{DeviceId, DisplayId, ProtocolVersion, ZonePreset};
use secrets::{MemoryStore, SecretKey, SecretStore};

const KEY_A: &str = "FakeLocalKeyAAA1";
const KEY_B: &str = "FakeLocalKeyBBB2";

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

/// Copia la fixture a una carpeta temporal, como si fuera `~/Documents/Open RGB`.
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

/// Contenido de todos los archivos de una carpeta, para verificar que no se tocó.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&p).unwrap(),
            )
        })
        .collect()
}

struct Env {
    _tmp: tempfile::TempDir,
    legacy: PathBuf,
    store: Store,
    secrets: MemoryStore,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = legacy_copy(tmp.path());
    let store = Store::open(tmp.path().join("datos de la app")).unwrap();
    Env {
        _tmp: tmp,
        legacy,
        store,
        secrets: MemoryStore::new(),
    }
}

fn dev(id: &str) -> DeviceId {
    DeviceId(id.to_owned())
}

fn key_of(secrets: &MemoryStore, id: &str) -> Option<String> {
    secrets
        .get(&SecretKey::DeviceLocalKey(dev(id)))
        .unwrap()
        .map(|s| s.expose().clone())
}

#[test]
fn importa_focos_claves_calibracion_y_estado_desde_una_copia() {
    let e = env();
    let before = snapshot(&e.legacy);
    let display = DisplayId("1".into());

    let report = import_legacy(&e.legacy, &e.store, &e.secrets, Some(&display)).unwrap();

    // Los dos focos, sin el enchufe
    assert_eq!(
        report.devices,
        vec![dev("bf00000000000000aaaa01"), dev("bf00000000000000bbbb02")]
    );
    assert_eq!(report.calibrations, 1);
    assert!(!report.settings_applied);
    assert!(report.warnings.contains(&WARN_SKIPPED_NON_LIGHT.to_owned()));
    // El segundo foco no trae versión (solo el de config.json la tiene)
    assert!(report.warnings.contains(&WARN_VERSION_ASSUMED.to_owned()));
    assert!(!report.warnings.contains(&WARN_NO_KEY.to_owned()));

    // Las claves están en el llavero
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000aaaa01").as_deref(),
        Some(KEY_A)
    );
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000bbbb02").as_deref(),
        Some(KEY_B)
    );
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000cccc03"),
        None,
        "el enchufe no se importa"
    );
    assert_eq!(e.secrets.len(), 2);

    // Los focos importados: lo de config.json (IP y versión) va al foco que usaba el script
    let devices = e.store.load_devices().unwrap();
    let a = devices
        .iter()
        .find(|d| d.id == dev("bf00000000000000aaaa01"))
        .unwrap();
    let b = devices
        .iter()
        .find(|d| d.id == dev("bf00000000000000bbbb02"))
        .unwrap();
    assert_eq!(a.name, "Foco de prueba A");
    assert_eq!(a.ip.as_deref(), Some("192.0.2.50"));
    assert!(!a.auto_ip);
    assert_eq!(a.version, ProtocolVersion::V35);
    assert_eq!(b.ip, None);
    assert!(b.auto_ip);
    assert_eq!(b.version, ProtocolVersion::V33, "se asume 3.3 y se avisó");
    // El mapeo de DPs viaja para que se resuelvan las capacidades
    let mapping = a.mapping.as_ref().unwrap();
    assert_eq!(mapping.0["24"].code, "colour_data_v2");
    assert!(b.mapping.as_ref().unwrap().0.contains_key("41"));
    // Zona por defecto: todo el monitor, como el script
    for d in [a, b] {
        assert_eq!(d.zones.len(), 1);
        assert_eq!(d.zones[0].preset, ZonePreset::Full);
        assert_eq!(d.zones[0].display, display);
    }

    // La calibración quedó en el foco del config.json y reproduce la del script
    let points = e
        .store
        .load_calibration(&dev("bf00000000000000aaaa01"))
        .unwrap()
        .unwrap();
    assert_eq!(points.len(), 12);
    assert_eq!(
        e.store
            .load_calibration(&dev("bf00000000000000bbbb02"))
            .unwrap(),
        None
    );
    let from_points = Calibration::from_points(&points);
    let from_script =
        Calibration::from_json(&fs::read_to_string(e.legacy.join("calibracion.json")).unwrap())
            .unwrap();
    for h in [0.0, 0.1, 0.35, 0.5, 0.77, 0.93] {
        assert_eq!(
            from_points.apply(h, 0.8),
            from_script.apply(h, 0.8),
            "tono {h}"
        );
    }

    // Última IP y estado original del foco
    let state = e.store.load_state().unwrap();
    let a_state = &state.devices[&dev("bf00000000000000aaaa01")];
    assert_eq!(a_state.last_ip.as_deref(), Some("192.0.2.51"));
    assert_eq!(a_state.original.as_ref().unwrap()["21"], "white");

    // Ninguna clave en los archivos de la app, ni en el informe
    for entry in fs::read_dir(e.store.dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let text = fs::read_to_string(&path).unwrap();
            assert!(
                !text.contains("FakeLocalKey"),
                "clave en {}",
                path.display()
            );
        }
    }
    assert!(!format!("{report:?}").contains("FakeLocalKey"));

    // La carpeta de origen quedó exactamente igual
    assert_eq!(snapshot(&e.legacy), before);
}

#[test]
fn sin_monitor_por_defecto_los_focos_nuevos_quedan_sin_zonas() {
    let e = env();
    import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert!(
        e.store
            .load_devices()
            .unwrap()
            .iter()
            .all(|d| d.zones.is_empty())
    );
}

#[test]
fn volver_a_importar_conserva_las_zonas_y_ajustes_del_usuario() {
    let e = env();
    let display = DisplayId("1".into());
    import_legacy(&e.legacy, &e.store, &e.secrets, Some(&display)).unwrap();

    // El usuario puso su zona y cambió la clave en el llavero
    let mut devices = e.store.load_devices().unwrap();
    devices[0].zones[0] = openlight_types::Zone::from_preset(display.clone(), ZonePreset::Left);
    e.store.save_devices(&devices).unwrap();
    e.secrets
        .set(
            &SecretKey::DeviceLocalKey(devices[0].id.clone()),
            &openlight_types::Secret::new("otra-clave".into()),
        )
        .unwrap();

    import_legacy(&e.legacy, &e.store, &e.secrets, Some(&display)).unwrap();
    let after = e.store.load_devices().unwrap();
    assert_eq!(after.len(), 2, "no duplica los focos");
    assert_eq!(
        after[0].zones[0].preset,
        ZonePreset::Left,
        "la zona del usuario se conserva"
    );
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000aaaa01").as_deref(),
        Some(KEY_A),
        "la clave se refresca"
    );
}

#[test]
fn solo_con_config_json_importa_el_foco_con_su_calibracion() {
    let e = env();
    fs::remove_file(e.legacy.join("devices.json")).unwrap();
    let report = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert_eq!(report.devices, vec![dev("bf00000000000000aaaa01")]);
    assert_eq!(report.calibrations, 1);
    assert!(
        !report.warnings.contains(&WARN_VERSION_ASSUMED.to_owned()),
        "config.json trae la versión"
    );
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000aaaa01").as_deref(),
        Some(KEY_A)
    );
    let devices = e.store.load_devices().unwrap();
    assert_eq!(
        devices[0].name, "bf00000000000000aaaa01",
        "sin nombre usa el id"
    );
}

#[test]
fn sin_config_json_la_calibracion_de_varios_focos_no_se_asigna_a_ciegas() {
    let e = env();
    fs::remove_file(e.legacy.join("config.json")).unwrap();
    let report = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert_eq!(report.devices.len(), 2);
    assert_eq!(report.calibrations, 0);
    assert!(
        report
            .warnings
            .contains(&WARN_CALIBRATION_UNASSIGNED.to_owned())
    );
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000bbbb02").as_deref(),
        Some(KEY_B)
    );
}

#[test]
fn con_un_solo_foco_la_calibracion_es_suya_aunque_no_haya_config_json() {
    let e = env();
    fs::remove_file(e.legacy.join("config.json")).unwrap();
    // Dejar solo el primer foco: el enchufe y el segundo foco se van
    let mut devices: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(e.legacy.join("devices.json")).unwrap()).unwrap();
    devices.truncate(1);
    fs::write(
        e.legacy.join("devices.json"),
        serde_json::to_string(&devices).unwrap(),
    )
    .unwrap();

    let report = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert_eq!(report.devices, vec![dev("bf00000000000000aaaa01")]);
    assert_eq!(report.calibrations, 1);
    assert!(
        e.store
            .load_calibration(&dev("bf00000000000000aaaa01"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn si_solo_hay_enchufes_no_hay_nada_que_importar() {
    let e = env();
    fs::remove_file(e.legacy.join("config.json")).unwrap();
    let mut devices: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(e.legacy.join("devices.json")).unwrap()).unwrap();
    devices.drain(..2);
    fs::write(
        e.legacy.join("devices.json"),
        serde_json::to_string(&devices).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        import_legacy(&e.legacy, &e.store, &e.secrets, None),
        Err(CoreError::Import(_))
    ));
    assert!(e.secrets.is_empty());
}

#[test]
fn un_foco_sin_clave_se_importa_con_aviso() {
    let e = env();
    fs::remove_file(e.legacy.join("config.json")).unwrap();
    let mut devices: Vec<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(e.legacy.join("devices.json")).unwrap()).unwrap();
    devices[0].as_object_mut().unwrap().remove("key");
    fs::write(
        e.legacy.join("devices.json"),
        serde_json::to_string(&devices).unwrap(),
    )
    .unwrap();

    let report = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert!(report.warnings.contains(&WARN_NO_KEY.to_owned()));
    assert_eq!(key_of(&e.secrets, "bf00000000000000aaaa01"), None);
    assert_eq!(
        key_of(&e.secrets, "bf00000000000000bbbb02").as_deref(),
        Some(KEY_B)
    );
}

#[test]
fn una_carpeta_sin_setup_o_rota_da_un_error_y_no_escribe_nada() {
    let e = env();
    let empty = e.legacy.parent().unwrap().join("vacia");
    fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        import_legacy(&empty, &e.store, &e.secrets, None),
        Err(CoreError::Import(_))
    ));

    // devices.json roto: el error no repite el contenido (que tiene claves)
    fs::write(
        e.legacy.join("devices.json"),
        r#"[{"id": "x", "key": "FakeLocalKeyZZZ9", "mapping": }]"#,
    )
    .unwrap();
    let error = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap_err();
    assert!(matches!(error, CoreError::Import(_)));
    assert!(!error.to_string().contains("FakeLocalKey"), "{error}");
    assert!(e.secrets.is_empty());
    assert!(e.store.load_devices().unwrap().is_empty());

    let visible = openlight_types::UserFacingError::from(error);
    assert_eq!(visible.code, openlight_types::ErrorCode::ImportInvalid);
}

#[test]
fn una_calibracion_rota_no_impide_importar_los_focos() {
    let e = env();
    fs::write(
        e.legacy.join("calibracion.json"),
        r#"{"no-es-un-tono": {"tono": 1}}"#,
    )
    .unwrap();
    let report = import_legacy(&e.legacy, &e.store, &e.secrets, None).unwrap();
    assert_eq!(report.devices.len(), 2);
    assert_eq!(report.calibrations, 0);
    assert!(
        report
            .warnings
            .contains(&"import.warning.calibration_invalid".to_owned())
    );
}
