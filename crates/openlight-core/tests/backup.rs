//! El respaldo sin claves: qué lleva, qué rechaza y qué pisa al importar. Todo con datos falsos.
#![allow(missing_docs)]

use std::collections::BTreeSet;

use openlight_core::backup::{
    self, Applied, Backup, Destination, FORMAT, VERSION, WARN_ZONES_MOVED,
};
use openlight_core::{Config, CoreError, DeviceRecord, Store};
use openlight_types::ipc::{CalibrationPoint, DuplicatePolicy};
use openlight_types::{
    Capabilities, ColourFormat, DeviceId, DisplayId, DpMapping, DpRange, DpSchema, Profile,
    ProfileId, ProtocolVersion, Region, SettingId, SettingValue, SettingsOverride, Zone,
    ZonePreset,
};
use serde_json::{Value, json};

const CREATED: &str = "2026-11-26T12:00:00Z";
const APP: &str = "9.9.9";
/// IPs de documentación (RFC 5737): cualquier aparición en el archivo es una fuga.
const FIXED_IP: &str = "192.0.2.77";
const LAST_IP: &str = "198.51.100.23";

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("datos")).unwrap();
    (dir, store)
}

fn dev(id: &str) -> DeviceId {
    DeviceId(id.to_owned())
}

fn profile_id(id: &str) -> ProfileId {
    ProfileId(id.to_owned())
}

fn values(items: &[(SettingId, f64)]) -> SettingsOverride {
    SettingsOverride(
        items
            .iter()
            .map(|(id, v)| (*id, SettingValue::Number(*v)))
            .collect(),
    )
}

fn caps() -> Capabilities {
    Capabilities {
        dp_switch: "20".into(),
        dp_mode: Some("21".into()),
        dp_bright: Some(DpRange {
            dp: "22".into(),
            min: 10,
            max: 1000,
            step: 1,
        }),
        dp_temp: None,
        dp_colour: Some(("24".into(), ColourFormat::V2Hsv12)),
        dp_control: None,
        modes: vec!["white".into(), "colour".into()],
    }
}

fn mapping() -> DpMapping {
    DpMapping(
        [(
            "20".to_owned(),
            DpSchema {
                code: "switch_led".into(),
                kind: "Boolean".into(),
                values: json!({}),
                raw_values: None,
            },
        )]
        .into(),
    )
}

fn zone(display: &str, preset: ZonePreset) -> Zone {
    Zone::from_preset(DisplayId(display.into()), preset)
}

fn record(id: &str) -> DeviceRecord {
    DeviceRecord {
        id: dev(id),
        name: format!("Foco {id}"),
        ip: Some(FIXED_IP.into()),
        auto_ip: false,
        version: ProtocolVersion::V35,
        mapping: Some(mapping()),
        capabilities: Some(caps()),
        zones: vec![zone("1", ZonePreset::Left)],
        overrides: values(&[(SettingId::BrightnessMax, 0.7)]),
    }
}

fn points(sat: f32) -> Vec<CalibrationPoint> {
    vec![CalibrationPoint {
        screen_hue_deg: 120.0,
        tono_deg: 100.0,
        sat,
    }]
}

fn factory() -> Vec<Profile> {
    vec![Profile {
        id: profile_id("movies"),
        name: "profile.movies".into(),
        values: values(&[(SettingId::SmoothingMs, 500.0)]),
        shortcut: Some("CmdOrCtrl+Alt+1".into()),
        builtin: true,
    }]
}

fn custom(id: &str, saturation: f64) -> Profile {
    Profile {
        id: profile_id(id),
        name: format!("Perfil {id}"),
        values: values(&[(SettingId::Saturation, saturation)]),
        shortcut: None,
        builtin: false,
    }
}

/// Una compu configurada: dos focos, uno calibrado y fuera del efecto, ajustes y perfiles.
fn source() -> (tempfile::TempDir, Store) {
    let (tmp, store) = store();
    store
        .save_devices(&[record("fake1"), record("fake2")])
        .unwrap();
    store
        .save_calibration(&dev("fake1"), &points(1.25))
        .unwrap();
    store.set_last_ip(&dev("fake1"), LAST_IP).unwrap();
    let mut movies = factory().remove(0);
    movies.values = values(&[(SettingId::SmoothingMs, 900.0)]);
    store
        .save_config(&Config {
            global: values(&[(SettingId::Saturation, 1.8)]),
            active_profile: Some(profile_id("cine")),
            profiles: vec![movies, custom("cine", 1.1)],
            cloud_region: Some(Region::Eu),
        })
        .unwrap();
    (tmp, store)
}

fn collect(store: &Store, excluded: &[DeviceId]) -> Backup {
    backup::collect(
        store,
        excluded,
        &|r| r.capabilities.clone(),
        CREATED.into(),
        APP.into(),
    )
    .unwrap()
}

/// El texto de un respaldo de la compu de [`source`].
fn exported() -> String {
    let (_tmp, store) = source();
    backup::render(&collect(&store, &[dev("fake1")])).unwrap()
}

/// El respaldo exportado, con un cambio hecho a mano.
fn tampered(change: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&exported()).unwrap();
    change(&mut value);
    value.to_string()
}

/// Una compu recién instalada: solo el perfil de fábrica.
fn fresh() -> (tempfile::TempDir, Store) {
    let (tmp, store) = store();
    store
        .save_config(&Config {
            profiles: factory(),
            ..Config::default()
        })
        .unwrap();
    (tmp, store)
}

fn apply(store: &Store, text: &str, policy: DuplicatePolicy) -> Applied {
    let displays = [DisplayId("1".into())];
    let factory = factory();
    backup::apply(
        backup::parse(text).unwrap(),
        store,
        policy,
        &Destination {
            factory_profiles: &factory,
            displays: &displays,
            fallback_display: displays.first(),
        },
    )
    .unwrap()
}

// ---------- exportar ----------

#[test]
fn el_respaldo_lleva_lo_configurado() {
    let (_tmp, store) = source();
    let backup = collect(&store, &[dev("fake1"), dev("yaquitado")]);

    assert_eq!(backup.format, FORMAT);
    assert_eq!(backup.version, VERSION);
    assert_eq!(backup.created_at, CREATED);
    assert_eq!(backup.app_version, APP);
    assert_eq!(
        backup.settings.global,
        values(&[(SettingId::Saturation, 1.8)])
    );
    assert_eq!(backup.settings.active_profile, Some(profile_id("cine")));
    assert_eq!(backup.profiles.len(), 2);
    // Solo los excluidos que siguen configurados
    assert_eq!(backup.excluded_devices, [dev("fake1")]);

    let first = &backup.devices[0];
    assert_eq!(first.id, dev("fake1"));
    assert_eq!(first.name, "Foco fake1");
    assert_eq!(first.version, ProtocolVersion::V35);
    assert_eq!(first.capabilities, Some(caps()));
    assert_eq!(first.zones, [zone("1", ZonePreset::Left)]);
    assert_eq!(first.overrides, values(&[(SettingId::BrightnessMax, 0.7)]));
    assert_eq!(first.calibration, Some(points(1.25)));
    assert_eq!(backup.devices[1].calibration, None);

    // Lo que se escribe se vuelve a leer igual
    assert_eq!(
        backup::parse(&backup::render(&backup).unwrap()).unwrap(),
        backup
    );
}

#[test]
fn la_version_del_protocolo_es_la_que_el_foco_anuncio() {
    let (_tmp, store) = source();
    store
        .update_state(|s| {
            s.devices.entry(dev("fake1")).or_default().version = Some(ProtocolVersion::V34);
        })
        .unwrap();
    assert_eq!(
        collect(&store, &[]).devices[0].version,
        ProtocolVersion::V34
    );
}

/// Los nombres de todos los campos de un JSON, en cualquier nivel.
fn field_names(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (name, field) in map {
                out.insert(name.clone());
                field_names(field, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| field_names(v, out)),
        _ => {}
    }
}

/// Lista cerrada: un campo nuevo en el respaldo hace fallar esta prueba, para que alguien mire
/// que no sea un secreto antes de sumarlo acá.
#[test]
fn los_campos_del_respaldo_son_exactamente_estos() {
    let mut names = BTreeSet::new();
    field_names(&serde_json::from_str(&exported()).unwrap(), &mut names);
    let allowed: BTreeSet<String> = [
        // el archivo
        "format",
        "version",
        "created_at",
        "app_version",
        "settings",
        "profiles",
        "devices",
        "excluded_devices",
        // ajustes y perfiles
        "global",
        "active_profile",
        "id",
        "name",
        "values",
        "shortcut",
        "builtin",
        // ids de ajustes que usa esta prueba
        "saturation",
        "smoothing_ms",
        "brightness_max",
        // foco
        "capabilities",
        "zones",
        "overrides",
        "calibration",
        // capacidades
        "dp_switch",
        "dp_mode",
        "dp_bright",
        "dp_temp",
        "dp_colour",
        "dp_control",
        "modes",
        "dp",
        "min",
        "max",
        "step",
        // zonas
        "display",
        "rect",
        "preset",
        "x",
        "y",
        "w",
        "h",
        // calibración
        "screen_hue_deg",
        "tono_deg",
        "sat",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(names, allowed);
}

#[test]
fn ni_las_ips_ni_la_region_de_la_nube_salen_en_el_archivo() {
    let text = exported();
    for leaked in [FIXED_IP, LAST_IP, "cloud_region", "switch_led"] {
        assert!(!text.contains(leaked), "{leaked} salió en {text}");
    }
}

/// Aunque alguien haya agregado una clave a mano en los archivos de la app, el tipo no tiene
/// dónde ponerla.
#[test]
fn un_campo_de_mas_en_los_archivos_de_la_app_no_llega_al_respaldo() {
    let (_tmp, store) = source();
    for (file, pointer) in [("devices.json", "/devices/0"), ("config.json", "")] {
        let path = store.dir().join(file);
        let mut value: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        value.pointer_mut(pointer).unwrap()["local_key"] = json!("COLADAlocalkey01");
        value.pointer_mut(pointer).unwrap()["access_secret"] = json!("COLADAsecret0001");
        std::fs::write(&path, value.to_string()).unwrap();
    }
    let text = backup::render(&collect(&store, &[])).unwrap();
    assert!(!text.contains("COLADA"), "{text}");
}

// ---------- leer ----------

fn rejected(text: &str) -> String {
    match backup::parse(text) {
        Err(CoreError::Backup(detail)) => detail,
        other => panic!("se esperaba un respaldo no válido y salió {other:?}"),
    }
}

#[test]
fn un_json_roto_dice_donde_esta_roto() {
    let detail = rejected("{\n  \"format\": ");
    assert!(detail.contains("línea 2"), "{detail}");
}

#[test]
fn un_archivo_que_no_es_un_respaldo_se_rechaza_sin_repetir_su_contenido() {
    // El `devices.json` del script: justo el archivo con claves que alguien podría elegir
    let legacy = r#"[{"id": "fake", "key": "SECRETAfakekey01"}]"#;
    for text in [
        legacy,
        "{}",
        "[]",
        "42",
        r#"{"format": "SECRETAfakekey01", "version": 1}"#,
        r#"{"format": "openlight-backup"}"#,
        r#"{"format": "openlight-backup", "version": "SECRETAfakekey01"}"#,
        r#"{"format": "openlight-backup", "version": 1, "created_at": 7, "app_version": "x"}"#,
        r#"{"format": "openlight-backup", "version": 1, "created_at": "x", "app_version": "x",
            "devices": "SECRETAfakekey01"}"#,
        "SECRETAfakekey01",
    ] {
        let detail = rejected(text);
        assert!(!detail.contains("SECRETA"), "{detail}");
    }
}

#[test]
fn una_version_desconocida_se_avisa_como_tal() {
    for version in [0, 2, 99] {
        let text = format!(r#"{{"format": "openlight-backup", "version": {version}}}"#);
        assert_eq!(
            backup::parse(&text),
            Err(CoreError::BackupVersion {
                found: version,
                supported: VERSION
            })
        );
    }
}

#[test]
fn se_lee_aunque_un_editor_le_haya_puesto_bom() {
    backup::parse(&format!("\u{feff}{}", exported())).unwrap();
}

#[test]
fn un_archivo_enorme_no_se_lee() {
    let text = format!("{}{}", exported(), " ".repeat(backup::MAX_BYTES));
    rejected(&text);
}

/// Un cambio hecho a mano en el archivo.
type Change = fn(&mut Value);

#[test]
fn un_valor_que_la_app_no_acepta_rechaza_todo_el_archivo() {
    let cases: [(&str, Change); 12] = [
        ("id que sale de la carpeta", |v| {
            v["devices"][0]["id"] = json!("../../SECRETA")
        }),
        ("foco repetido", |v| {
            let copy = v["devices"][0].clone();
            v["devices"].as_array_mut().unwrap().push(copy);
        }),
        ("foco sin nombre", |v| v["devices"][0]["name"] = json!("  ")),
        ("zona fuera del monitor", |v| {
            v["devices"][0]["zones"][0]["rect"]["w"] = json!(3.0)
        }),
        ("demasiadas zonas", |v| {
            let zone = v["devices"][0]["zones"][0].clone();
            v["devices"][0]["zones"] = Value::Array(vec![zone; 9]);
        }),
        ("ajuste del foco fuera de rango", |v| {
            v["devices"][0]["overrides"]["brightness_max"] = json!(7)
        }),
        ("ajuste global desconocido", |v| {
            v["settings"]["global"]["SECRETA"] = json!(1)
        }),
        ("opción que no existe", |v| {
            v["settings"]["global"]["color_mode"] = json!("SECRETA")
        }),
        ("ajuste de perfil fuera de rango", |v| {
            v["profiles"][1]["values"]["saturation"] = json!(-4)
        }),
        ("perfil repetido", |v| {
            let copy = v["profiles"][1].clone();
            v["profiles"].as_array_mut().unwrap().push(copy);
        }),
        ("perfil sin id", |v| v["profiles"][1]["id"] = json!("")),
        ("calibración que no es un número", |v| {
            // 1e39 entra en un f64 y no en un f32: queda infinito
            v["devices"][0]["calibration"][0]["sat"] = json!(1e39)
        }),
    ];
    for (name, change) in cases {
        let text = tampered(change);
        match backup::parse(&text) {
            Err(CoreError::Backup(detail)) => {
                assert!(!detail.contains("SECRETA"), "{name}: {detail}");
            }
            other => panic!("{name}: se esperaba un rechazo y salió {other:?}"),
        }
    }
}

// ---------- resumir ----------

#[test]
fn el_resumen_separa_los_focos_nuevos_de_los_repetidos() {
    let backup = backup::parse(&exported()).unwrap();
    let mut here = record("fake2");
    here.name = "Otro nombre acá".into();
    let summary = backup::summarize(&backup, &[here, record("soloaca")]);
    assert_eq!(summary.created_at, CREATED);
    assert_eq!(summary.app_version, APP);
    assert_eq!(summary.new_devices.len(), 1);
    assert_eq!(summary.new_devices[0].id, dev("fake1"));
    assert_eq!(summary.new_devices[0].name, "Foco fake1");
    assert_eq!(summary.existing_devices.len(), 1);
    assert_eq!(summary.existing_devices[0].id, dev("fake2"));
}

// ---------- aplicar ----------

#[test]
fn en_una_compu_nueva_queda_todo_menos_las_ips() {
    let (_tmp, store) = fresh();
    let applied = apply(&store, &exported(), DuplicatePolicy::Skip);

    assert_eq!(applied.added, [dev("fake1"), dev("fake2")]);
    assert!(applied.replaced.is_empty());
    assert_eq!(applied.report.devices, applied.added);
    assert_eq!(applied.report.calibrations, 1);
    assert!(applied.report.settings_applied);
    assert!(applied.report.warnings.is_empty(), "{applied:?}");
    assert_eq!(applied.excluded, [dev("fake1")]);

    let devices = store.load_devices().unwrap();
    let mut expected = record("fake1");
    // Sin IP: se busca sola. El mapeo vuelve junto con la clave
    expected.ip = None;
    expected.auto_ip = true;
    expected.mapping = None;
    assert_eq!(devices[0], expected);
    assert_eq!(
        store.load_calibration(&dev("fake1")).unwrap(),
        Some(points(1.25))
    );

    let config = store.load_config().unwrap();
    assert_eq!(config.global, values(&[(SettingId::Saturation, 1.8)]));
    assert_eq!(config.active_profile, Some(profile_id("cine")));
    assert_eq!(config.profiles.len(), 2);
    // El perfil de fábrica estaba sin tocar: no es "tuyo", así que llega el del respaldo
    assert_eq!(
        config.profiles[0].values,
        values(&[(SettingId::SmoothingMs, 900.0)])
    );
    // La región de la nube llega al cargar las credenciales
    assert_eq!(config.cloud_region, None);
}

/// Una compu que ya tiene `fake1` configurado a su manera, ajustes propios y algo que el
/// respaldo no conoce.
fn used() -> (tempfile::TempDir, Store) {
    let (tmp, store) = store();
    let mut mine = record("fake1");
    mine.name = "El mío".into();
    mine.ip = Some("203.0.113.5".into());
    mine.auto_ip = false;
    mine.version = ProtocolVersion::V33;
    mine.zones = vec![zone("1", ZonePreset::Right)];
    mine.overrides = values(&[(SettingId::Saturation, 2.0)]);
    store.save_devices(&[mine, record("soloaca")]).unwrap();
    store.save_calibration(&dev("fake1"), &points(0.5)).unwrap();
    let mut movies = factory().remove(0);
    movies.values = values(&[(SettingId::SmoothingMs, 200.0)]);
    store
        .save_config(&Config {
            global: values(&[(SettingId::CaptureFps, 30.0)]),
            active_profile: None,
            profiles: vec![movies, custom("cine", 2.2), custom("propio", 1.5)],
            cloud_region: Some(Region::Us),
        })
        .unwrap();
    (tmp, store)
}

#[test]
fn saltear_no_toca_nada_de_lo_que_ya_estaba() {
    let (_tmp, store) = used();
    let before_devices = store.load_devices().unwrap();
    let before_config = store.load_config().unwrap();

    let applied = apply(&store, &exported(), DuplicatePolicy::Skip);

    assert_eq!(applied.added, [dev("fake2")]);
    assert!(applied.replaced.is_empty());
    assert_eq!(applied.report.devices, [dev("fake2")]);
    // `fake1` está excluido en el respaldo, pero acá no se importó: no se le cambia nada
    assert!(applied.excluded.is_empty());
    assert_eq!(applied.report.calibrations, 0);
    assert!(!applied.report.settings_applied);

    let devices = store.load_devices().unwrap();
    assert_eq!(devices[..2], before_devices[..]);
    assert_eq!(devices[2].id, dev("fake2"));
    assert_eq!(
        store.load_calibration(&dev("fake1")).unwrap(),
        Some(points(0.5))
    );
    assert_eq!(store.load_config().unwrap(), before_config);
}

#[test]
fn saltear_igual_suma_los_perfiles_que_faltaban() {
    let (_tmp, store) = fresh();
    store
        .save_config(&Config {
            global: values(&[(SettingId::CaptureFps, 30.0)]),
            profiles: factory(),
            ..Config::default()
        })
        .unwrap();
    let applied = apply(&store, &exported(), DuplicatePolicy::Skip);
    assert!(!applied.report.settings_applied);
    let config = store.load_config().unwrap();
    assert_eq!(config.global, values(&[(SettingId::CaptureFps, 30.0)]));
    assert_eq!(config.active_profile, None);
    assert!(config.profiles.iter().any(|p| p.id == profile_id("cine")));
}

#[test]
fn pisar_usa_lo_del_respaldo_y_conserva_lo_que_es_de_esta_compu() {
    let (_tmp, store) = used();
    let applied = apply(&store, &exported(), DuplicatePolicy::Replace);

    assert_eq!(applied.added, [dev("fake2")]);
    assert_eq!(applied.replaced, [dev("fake1")]);
    assert_eq!(applied.report.devices, [dev("fake1"), dev("fake2")]);
    assert_eq!(applied.excluded, [dev("fake1")]);
    assert!(applied.report.settings_applied);

    let devices = store.load_devices().unwrap();
    assert_eq!(devices.len(), 3, "no se borra ningún foco");
    let mine = &devices[0];
    assert_eq!(mine.name, "Foco fake1");
    assert_eq!(mine.version, ProtocolVersion::V35);
    assert_eq!(mine.zones, [zone("1", ZonePreset::Left)]);
    assert_eq!(mine.overrides, values(&[(SettingId::BrightnessMax, 0.7)]));
    // La IP y el mapeo son de esta compu: el respaldo no los trae
    assert_eq!(mine.ip.as_deref(), Some("203.0.113.5"));
    assert!(!mine.auto_ip);
    assert_eq!(mine.mapping, Some(mapping()));
    assert_eq!(
        store.load_calibration(&dev("fake1")).unwrap(),
        Some(points(1.25))
    );

    let config = store.load_config().unwrap();
    assert_eq!(config.global, values(&[(SettingId::Saturation, 1.8)]));
    assert_eq!(config.active_profile, Some(profile_id("cine")));
    assert_eq!(config.cloud_region, Some(Region::Us));
    let find = |id: &str| {
        config
            .profiles
            .iter()
            .find(|p| p.id == profile_id(id))
            .unwrap()
    };
    assert_eq!(find("cine").values, values(&[(SettingId::Saturation, 1.1)]));
    assert_eq!(
        find("movies").values,
        values(&[(SettingId::SmoothingMs, 900.0)])
    );
    assert_eq!(
        find("propio").values,
        values(&[(SettingId::Saturation, 1.5)])
    );
}

#[test]
fn pisar_nunca_borra_una_calibracion_ni_las_capacidades() {
    let (_tmp, store) = used();
    store.save_calibration(&dev("fake1"), &points(0.5)).unwrap();
    let text = tampered(|v| {
        v["devices"][0]["calibration"] = Value::Null;
        v["devices"][0]["capabilities"] = Value::Null;
    });
    apply(&store, &text, DuplicatePolicy::Replace);
    assert_eq!(
        store.load_calibration(&dev("fake1")).unwrap(),
        Some(points(0.5))
    );
    assert_eq!(store.load_devices().unwrap()[0].capabilities, Some(caps()));
}

#[test]
fn un_perfil_importado_nunca_se_vuelve_de_fabrica() {
    let (_tmp, store) = fresh();
    let text = tampered(|v| v["profiles"][1]["builtin"] = json!(true));
    apply(&store, &text, DuplicatePolicy::Replace);
    let config = store.load_config().unwrap();
    assert!(!config.profiles[1].builtin);
    assert!(config.profiles[0].builtin);
    assert_eq!(config.profiles[0].name, "profile.movies");
}

#[test]
fn un_perfil_activo_que_no_existe_se_ignora() {
    let (_tmp, store) = fresh();
    let text = tampered(|v| v["settings"]["active_profile"] = json!("no-existe"));
    apply(&store, &text, DuplicatePolicy::Replace);
    assert_eq!(store.load_config().unwrap().active_profile, None);
}

#[test]
fn una_zona_de_un_monitor_que_no_esta_pasa_al_de_esta_compu() {
    let (_tmp, store) = fresh();
    let text = tampered(|v| v["devices"][0]["zones"][0]["display"] = json!("69733382"));
    let applied = apply(&store, &text, DuplicatePolicy::Skip);
    assert_eq!(applied.report.warnings, [WARN_ZONES_MOVED]);
    assert_eq!(
        store.load_devices().unwrap()[0].zones,
        [zone("1", ZonePreset::Left)]
    );
}

#[test]
fn sin_poder_listar_los_monitores_las_zonas_quedan_como_venian() {
    let (_tmp, store) = fresh();
    let factory = factory();
    let applied = backup::apply(
        backup::parse(&exported()).unwrap(),
        &store,
        DuplicatePolicy::Skip,
        &Destination {
            factory_profiles: &factory,
            displays: &[],
            fallback_display: None,
        },
    )
    .unwrap();
    assert!(applied.report.warnings.is_empty());
    assert_eq!(
        store.load_devices().unwrap()[0].zones,
        [zone("1", ZonePreset::Left)]
    );
}
