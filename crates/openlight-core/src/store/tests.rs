use openlight_types::{DisplayId, SettingId, SettingValue, ZonePreset};

use super::*;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("datos")).unwrap();
    (dir, store)
}

fn dev(id: &str) -> DeviceId {
    DeviceId(id.to_owned())
}

fn record(id: &str) -> DeviceRecord {
    DeviceRecord {
        id: dev(id),
        name: format!("Foco {id}"),
        ip: Some("192.0.2.10".into()),
        auto_ip: false,
        version: ProtocolVersion::V35,
        mapping: None,
        capabilities: None,
        zones: vec![Zone::from_preset(DisplayId("1".into()), ZonePreset::Left)],
        overrides: SettingsOverride::default(),
        order: 0,
    }
}

fn ordered(id: &str, name: &str, order: u32) -> DeviceRecord {
    DeviceRecord {
        name: name.to_owned(),
        order,
        ..record(id)
    }
}

fn ids(devices: &[DeviceRecord]) -> Vec<&str> {
    devices.iter().map(|d| d.id.0.as_str()).collect()
}

fn number(value: f64) -> SettingValue {
    SettingValue::Number(value)
}

#[test]
fn sin_archivos_todo_es_por_defecto() {
    let (_tmp, store) = store();
    assert_eq!(store.load_config().unwrap(), Config::default());
    assert!(store.load_devices().unwrap().is_empty());
    assert_eq!(store.load_calibration(&dev("fake1")).unwrap(), None);
    assert_eq!(store.load_state().unwrap(), State::default());
}

#[test]
fn la_config_ida_y_vuelta_lleva_version() {
    let (_tmp, store) = store();
    let mut config = Config::default();
    config.global.0.insert(SettingId::Saturation, number(1.8));
    config.cloud_region = Some(Region::Eu);
    store.save_config(&config).unwrap();

    let text = fs::read_to_string(store.dir().join("config.json")).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["version"], CONFIG_VERSION);
    assert_eq!(store.load_config().unwrap(), config);
}

#[test]
fn no_guarda_ajustes_fuera_de_rango() {
    let (_tmp, store) = store();
    let mut config = Config::default();
    config.global.0.insert(SettingId::Saturation, number(99.0));
    assert!(matches!(
        store.save_config(&config),
        Err(CoreError::Invalid(_))
    ));
    assert!(!store.dir().join("config.json").exists());
}

#[test]
fn migra_la_config_v0() {
    let (_tmp, store) = store();
    fs::write(
        store.dir().join("config.json"),
        r#"{"settings": {"saturation": 2.0}}"#,
    )
    .unwrap();
    let config = store.load_config().unwrap();
    assert_eq!(
        config.global.0.get(&SettingId::Saturation),
        Some(&number(2.0))
    );
    // Al guardar queda en la versión actual y con el nombre nuevo
    store.save_config(&config).unwrap();
    let json: Value =
        serde_json::from_str(&fs::read_to_string(store.dir().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(json["version"], CONFIG_VERSION);
    assert!(json.get("settings").is_none());
}

#[test]
fn rechaza_un_archivo_de_una_version_futura() {
    let (_tmp, store) = store();
    fs::write(store.dir().join("config.json"), r#"{"version": 99}"#).unwrap();
    assert!(matches!(
        store.load_config(),
        Err(CoreError::UnsupportedVersion { found: 99, .. })
    ));
}

#[test]
fn un_json_roto_da_un_error_con_el_archivo() {
    let (_tmp, store) = store();
    fs::write(store.dir().join("state.json"), "{no es json").unwrap();
    let error = store.load_state().unwrap_err();
    assert!(matches!(&error, CoreError::Json { path, .. } if path.ends_with("state.json")));
}

#[test]
fn los_focos_se_guardan_sin_claves() {
    let (_tmp, store) = store();
    let devices = vec![record("fake1"), record("fake2")];
    store.save_devices(&devices).unwrap();
    assert_eq!(store.load_devices().unwrap(), devices);
    let text = fs::read_to_string(store.dir().join("devices.json")).unwrap();
    assert!(
        !text.contains("key"),
        "devices.json no puede tener claves: {text}"
    );
}

#[test]
fn la_calibracion_es_por_foco() {
    let (_tmp, store) = store();
    let points = vec![
        CalibrationPoint {
            screen_hue_deg: 0.0,
            tono_deg: 0.0,
            sat: 1.0,
        },
        CalibrationPoint {
            screen_hue_deg: 120.0,
            tono_deg: 100.0,
            sat: 1.25,
        },
    ];
    store.save_calibration(&dev("fake1"), &points).unwrap();
    assert_eq!(store.load_calibration(&dev("fake1")).unwrap(), Some(points));
    assert_eq!(store.load_calibration(&dev("fake2")).unwrap(), None);
    store.delete_calibration(&dev("fake1")).unwrap();
    store.delete_calibration(&dev("fake1")).unwrap();
    assert_eq!(store.load_calibration(&dev("fake1")).unwrap(), None);
}

#[test]
fn un_id_raro_no_puede_salir_de_la_carpeta() {
    let (_tmp, store) = store();
    for id in ["../config", "a/b", "", "a b", "ñu"] {
        assert!(
            matches!(
                store.save_calibration(&dev(id), &[]),
                Err(CoreError::Invalid(_))
            ),
            "{id:?}"
        );
    }
}

#[test]
fn el_estado_recuerda_ip_y_original() {
    let (_tmp, store) = store();
    let mut original = Dps::new();
    original.insert("20".into(), Value::Bool(true));
    original.insert("21".into(), Value::String("white".into()));

    store.set_last_ip(&dev("fake1"), "192.0.2.20").unwrap();
    store.set_original(&dev("fake1"), original.clone()).unwrap();
    store.set_last_ip(&dev("fake2"), "192.0.2.21").unwrap();

    let state = store.load_state().unwrap();
    assert_eq!(
        state.devices[&dev("fake1")].last_ip.as_deref(),
        Some("192.0.2.20")
    );
    assert_eq!(state.devices[&dev("fake1")].original, Some(original));

    store.clear_original(&dev("fake1")).unwrap();
    let state = store.load_state().unwrap();
    assert_eq!(state.devices[&dev("fake1")].original, None);
    // lo del otro foco no se tocó
    assert_eq!(
        state.devices[&dev("fake2")].last_ip.as_deref(),
        Some("192.0.2.21")
    );
}

#[test]
fn escribir_no_deja_temporales() {
    let (_tmp, store) = store();
    store.save_devices(&[record("fake1")]).unwrap();
    store.set_last_ip(&dev("fake1"), "192.0.2.20").unwrap();
    let names: Vec<_> = fs::read_dir(store.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
}

#[test]
fn ajustes_efectivos_apilan_globales_perfil_y_foco() {
    let mut config = Config::default();
    config.global.0.insert(SettingId::Saturation, number(1.5));
    config
        .global
        .0
        .insert(SettingId::BrightnessMax, number(0.9));
    let mut profile_values = SettingsOverride::default();
    profile_values
        .0
        .insert(SettingId::BrightnessMax, number(0.6));
    config.profiles.push(Profile {
        id: ProfileId("movies".into()),
        name: "profile.movies".into(),
        values: profile_values,
        shortcut: None,
        builtin: true,
    });
    config.active_profile = Some(ProfileId("movies".into()));
    let mut device = SettingsOverride::default();
    device.0.insert(SettingId::MaxSendsPerS, number(3.0));

    let s = config.effective_settings(Some(&device)).unwrap();
    assert_eq!(s.saturation, 1.5, "global");
    assert_eq!(s.brightness_max, 0.6, "el perfil pisa el global");
    assert_eq!(s.max_sends_per_s, 3.0, "el foco pisa todo");
    assert_eq!(s.capture_fps, Settings::default().capture_fps);
}

// ---------- Rango de brillo ----------

#[test]
fn un_brillo_maximo_en_cero_guardado_antes_se_sigue_leyendo() {
    let (_tmp, store) = store();
    fs::write(
        store.dir().join("config.json"),
        r#"{"version": 1, "global": {"brightness_max": 0.0},
            "profiles": [{"id": "mio", "name": "Mío", "values": {"brightness_max": 0.01},
                          "shortcut": null, "builtin": false}]}"#,
    )
    .unwrap();
    fs::write(
        store.dir().join("devices.json"),
        r#"{"version": 1, "devices": [{"id": "fake1", "name": "Foco", "version": "3.5",
            "overrides": {"brightness_max": 0.0}}]}"#,
    )
    .unwrap();
    let config = store.load_config().unwrap();
    assert_eq!(config.global.0[&SettingId::BrightnessMax], number(0.05));
    assert_eq!(
        config.profiles[0].values.0[&SettingId::BrightnessMax],
        number(0.05)
    );
    let devices = store.load_devices().unwrap();
    assert_eq!(
        devices[0].overrides.0[&SettingId::BrightnessMax],
        number(0.05)
    );
    // Y se puede volver a guardar
    store.save_config(&config).unwrap();
    store.save_devices(&devices).unwrap();
}

#[test]
fn con_el_minimo_por_encima_del_maximo_manda_el_maximo() {
    // Puede pasar al combinar capas: el mínimo global alto con el máximo bajo de un perfil
    let mut config = Config::default();
    config
        .global
        .0
        .insert(SettingId::BrightnessMin, number(0.7));
    let mut own = SettingsOverride::default();
    own.0.insert(SettingId::BrightnessMax, number(0.5));
    let settings = config.effective_settings(Some(&own)).unwrap();
    assert_eq!(settings.brightness_max, 0.5);
    assert_eq!(settings.brightness_min, 0.5);
}

// ---------- Orden de los focos ----------

#[test]
fn los_focos_salen_por_orden_y_no_por_como_se_guardaron() {
    let (_tmp, store) = store();
    store
        .save_devices(&[
            ordered("a", "Primero", 2),
            ordered("b", "Segundo", 0),
            ordered("c", "Tercero", 1),
        ])
        .unwrap();
    assert_eq!(ids(&store.load_devices().unwrap()), ["b", "c", "a"]);
}

#[test]
fn con_el_mismo_orden_desempata_el_nombre_sin_mirar_mayusculas() {
    let (_tmp, store) = store();
    store
        .save_devices(&[
            ordered("a", "velador", 0),
            ordered("b", "Techo", 0),
            ordered("c", "escritorio", 0),
        ])
        .unwrap();
    assert_eq!(ids(&store.load_devices().unwrap()), ["c", "b", "a"]);
}

#[test]
fn un_devices_json_sin_el_campo_se_sigue_leyendo() {
    let (_tmp, store) = store();
    fs::write(
        store.dir().join("devices.json"),
        r#"{"version": 1, "devices": [
            {"id": "fake1", "name": "Velador", "version": "3.5"},
            {"id": "fake2", "name": "Techo", "version": "3.3"}]}"#,
    )
    .unwrap();
    let devices = store.load_devices().unwrap();
    assert!(devices.iter().all(|d| d.order == 0));
    assert_eq!(ids(&devices), ["fake2", "fake1"]);
}

#[test]
fn un_foco_nuevo_va_despues_de_todos() {
    assert_eq!(next_order(&[]), 0);
    let devices = [ordered("a", "A", 4), ordered("b", "B", 1)];
    assert_eq!(next_order(&devices), 5);
    // Un archivo de antes, con todos en cero: el nuevo igual queda último
    assert_eq!(next_order(&[ordered("a", "A", 0)]), 1);
    assert_eq!(next_order(&[ordered("a", "A", u32::MAX)]), u32::MAX);
}

#[test]
fn reordenar_cambia_el_orden_y_nada_mas() {
    let (_tmp, store) = store();
    store
        .save_devices(&[
            ordered("a", "A", 0),
            ordered("b", "B", 1),
            ordered("c", "C", 2),
        ])
        .unwrap();
    store
        .reorder_devices(&[dev("c"), dev("a"), dev("b")])
        .unwrap();
    let devices = store.load_devices().unwrap();
    assert_eq!(
        devices,
        [
            ordered("c", "C", 0),
            ordered("a", "A", 1),
            ordered("b", "B", 2),
        ]
    );
}

#[test]
fn reordenar_exige_exactamente_los_focos_configurados() {
    let (_tmp, store) = store();
    let before = [ordered("a", "A", 0), ordered("b", "B", 1)];
    store.save_devices(&before).unwrap();
    for (list, why) in [
        (vec![dev("b")], "falta"),
        (vec![dev("b"), dev("a"), dev("x")], "sobra"),
        (vec![dev("b"), dev("x")], "otro"),
        (vec![dev("b"), dev("b")], "repetido"),
        (vec![dev("b"), dev("a"), dev("a")], "repetido y de más"),
        (vec![], "vacía"),
    ] {
        assert!(
            matches!(
                store.reorder_devices(&list),
                Err(CoreError::DeviceOrderMismatch)
            ),
            "{why}"
        );
        assert_eq!(
            store.load_devices().unwrap(),
            before,
            "{why}: no se tocó nada"
        );
    }
}

#[test]
fn reordenar_sin_focos_acepta_una_lista_vacia() {
    let (_tmp, store) = store();
    store.reorder_devices(&[]).unwrap();
    assert!(store.load_devices().unwrap().is_empty());
}
