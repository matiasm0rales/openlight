//! El respaldo sin claves, con el backend de verdad: ida y vuelta entre dos "compus" (cada una
//! con su carpeta de datos temporal y su llavero en memoria), que ningún secreto ni IP salga en el
//! archivo, y bajar las claves de la nube falsa. Nunca toca el llavero ni la red reales.
#![allow(missing_docs)]

mod common;

use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{
    FAKE_ACCESS_ID, FAKE_ACCESS_SECRET, Harness, Loopback, Overrides, Setup, eventually, harness,
    harness_overriding, sim_id,
};
use openlight_lib::events::AppEvent;
use openlight_types::ipc::{
    CalibrationPoint, CloudCredentialsInput, DeviceAddRequest, DuplicatePolicy, SettingScope,
};
use openlight_types::{
    DeviceId, ErrorCode, FixAction, ProfileId, Region, SessionState, SettingId, SettingValue, Zone,
    ZonePreset,
};
use secrets::{SecretKey, SecretStore};
use serde_json::{Value, json};

const LOCAL_KEY: &[u8; 16] = b"zLk!9fUgA2qW7xPm";
const SECOND_KEY: &str = "qR8#vTn2Lp5!cWd1";
const ACCESS_ID: &str = "fugaid7Qm2Zk9XwR4tLp";
const ACCESS_SECRET: &str = "fugasecretb81Lk0aZ9QmN3vTy7Wc5Hd";
/// IPs de documentación (RFC 5737).
const FIXED_IP: &str = "192.0.2.77";
const LAST_IP: &str = "198.51.100.23";

/// La compu a la que se llevan los focos: sin nada guardado, y con los focos en la red.
async fn new_computer(sims: usize) -> Harness {
    harness_overriding(
        Setup { sims, lan: true },
        Overrides {
            locator: Some(std::sync::Arc::new(Loopback)),
            ..Overrides::default()
        },
    )
    .await
}

async fn credentials(h: &Harness) {
    h.backend
        .cloud_validate(CloudCredentialsInput {
            access_id: FAKE_ACCESS_ID.into(),
            access_secret: FAKE_ACCESS_SECRET.into(),
            region: Region::Us,
        })
        .await
        .unwrap();
}

/// Una compu configurada de punta a punta: dos focos de la cuenta, uno con zona, ajuste propio,
/// calibración y fuera del efecto; ajustes globales y un perfil propio activo.
async fn configure(h: &Harness) {
    let b = &h.backend;
    credentials(h).await;
    for n in [1, 2] {
        b.device_add(DeviceAddRequest {
            id: sim_id(n),
            name: format!("Mi foco {n}"),
            ip: None,
        })
        .await
        .unwrap();
    }
    let display = b.displays_list().unwrap().remove(0).id;
    b.zones_set(
        sim_id(1),
        vec![Zone::from_preset(display.clone(), ZonePreset::Left)],
    )
    .unwrap();
    let number = SettingValue::Number;
    b.settings_set(
        SettingScope::Device(sim_id(1)),
        SettingId::BrightnessMax,
        number(0.6),
    )
    .unwrap();
    b.settings_set(SettingScope::Global, SettingId::Saturation, number(1.7))
        .unwrap();
    let profile = b.profiles_create("Cine en casa".into(), None).unwrap();
    b.settings_set(
        SettingScope::Profile(profile.id.clone()),
        SettingId::SmoothingMs,
        number(700.0),
    )
    .unwrap();
    b.profiles_activate(Some(profile.id)).unwrap();
    b.calibration_start(sim_id(1), display, true).await.unwrap();
    b.calibration_set(1, Some(70.0), Some(1.2)).await.unwrap();
    b.calibration_save().await.unwrap();
    b.set_device_in_effect(sim_id(1), false).await.unwrap();
}

/// Cambia nombre e IP fija de un foco (una IP de documentación: ahí no contesta nadie).
fn rename_with_fixed_ip(h: &Harness, id: DeviceId, name: &str) {
    h.backend
        .device_update(DeviceAddRequest {
            id,
            name: name.into(),
            ip: Some(FIXED_IP.into()),
        })
        .unwrap();
}

fn calibration(dir: &Path, id: &DeviceId) -> Vec<CalibrationPoint> {
    openlight_core::Store::open(dir)
        .unwrap()
        .load_calibration(id)
        .unwrap()
        .expect("sin calibración")
}

fn key_of(h: &Harness, id: &DeviceId) -> Option<String> {
    h.secrets
        .get(&SecretKey::DeviceLocalKey(id.clone()))
        .unwrap()
        .map(|k| k.expose().clone())
}

#[tokio::test]
async fn ida_y_vuelta_a_otra_compu_y_las_claves_bajan_de_la_nube() {
    let two = || Setup { sims: 2, lan: true };
    let old = harness(two()).await;
    configure(&old).await;
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("respaldo.json");
    old.backend.backup_save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();

    // La otra compu: carpeta de datos y llavero vacíos
    let new = new_computer(2).await;
    let b = &new.backend;
    let summary = b.backup_inspect(&text).unwrap();
    assert_eq!(summary.new_devices.len(), 2);
    assert!(summary.existing_devices.is_empty());
    assert_eq!(summary.app_version, env!("CARGO_PKG_VERSION"));
    assert!(
        summary.created_at.ends_with('Z') && summary.created_at.len() == 20,
        "{}",
        summary.created_at
    );
    assert!(b.devices_list().unwrap().is_empty(), "mirar no cambia nada");

    let report = b.backup_import(&text, DuplicatePolicy::Skip).await.unwrap();
    assert_eq!(report.devices, [sim_id(1), sim_id(2)]);
    assert_eq!(report.calibrations, 1);
    assert!(report.settings_applied);

    let mut expected = old.backend.devices_list().unwrap();
    for device in &mut expected {
        // Sin IPs: se buscan solas
        device.ip = None;
        device.auto_ip = true;
    }
    assert_eq!(b.devices_list().unwrap(), expected);
    assert!(!b.devices_list().unwrap()[0].in_effect);
    assert_eq!(
        b.settings_get().unwrap(),
        old.backend.settings_get().unwrap()
    );
    assert_eq!(
        b.profiles_list().unwrap(),
        old.backend.profiles_list().unwrap()
    );
    assert_eq!(
        calibration(b.data_dir(), &sim_id(1)),
        calibration(old.backend.data_dir(), &sim_id(1))
    );
    // La exclusión sobrevive a reabrir la app
    assert!(!common::reopen(&new).devices_list().unwrap()[0].in_effect);

    // Sin claves y sin credenciales: quedan pendientes, con un error claro
    assert_eq!(b.devices_without_key().unwrap(), [sim_id(1), sim_id(2)]);
    let error = b.fetch_missing_keys().await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CloudNoCredentials);
    assert!(!error.actions.is_empty());
    assert_eq!(b.devices_without_key().unwrap().len(), 2);

    credentials(&new).await;
    let fetched = b.fetch_missing_keys().await.unwrap();
    assert_eq!(fetched.fetched, [sim_id(1), sim_id(2)]);
    assert!(fetched.not_found.is_empty());
    assert!(b.devices_without_key().unwrap().is_empty());
    for (n, sim) in new.sims.iter().enumerate() {
        assert_eq!(
            key_of(&new, &sim_id(n + 1)).unwrap().as_bytes(),
            sim.local_key()
        );
    }
    // Con la clave volvió el mapeo: las escenas dependen de él
    let records = openlight_core::Store::open(b.data_dir())
        .unwrap()
        .load_devices()
        .unwrap();
    assert!(records.iter().all(|r| r.mapping.is_some()));
    // Y el foco responde
    b.device_test(sim_id(2), openlight_types::ipc::TestPattern::Blink)
        .await
        .unwrap();
    // Sin nada pendiente no hace falta la nube
    assert_eq!(b.fetch_missing_keys().await.unwrap(), Default::default());
}

#[tokio::test]
async fn el_orden_de_los_focos_viaja_con_el_respaldo() {
    let old = harness(Setup { sims: 2, lan: true }).await;
    configure(&old).await;
    let ids = |h: &Harness| -> Vec<DeviceId> {
        h.backend
            .devices_list()
            .unwrap()
            .into_iter()
            .map(|d| d.id)
            .collect()
    };
    // Al revés que por nombre
    let arranged = vec![sim_id(2), sim_id(1)];
    old.backend.devices_reorder(arranged.clone()).unwrap();
    let text = old.backend.backup_export().unwrap();

    let new = new_computer(2).await;
    let b = &new.backend;
    b.backup_import(&text, DuplicatePolicy::Skip).await.unwrap();
    assert_eq!(ids(&new), arranged);

    let mine = vec![sim_id(1), sim_id(2)];
    b.devices_reorder(mine.clone()).unwrap();
    b.backup_import(&text, DuplicatePolicy::Skip).await.unwrap();
    assert_eq!(ids(&new), mine, "saltear no reordena");
    b.backup_import(&text, DuplicatePolicy::Replace)
        .await
        .unwrap();
    assert_eq!(ids(&new), arranged, "pisar vuelve al orden del respaldo");
}

#[tokio::test]
async fn un_foco_que_no_esta_en_la_cuenta_sigue_pendiente_y_se_dice_cual() {
    let old = harness(Setup { sims: 2, lan: true }).await;
    configure(&old).await;
    let text = old.backend.backup_export().unwrap();
    // La cuenta de la otra compu solo tiene el primero
    let new = new_computer(1).await;
    new.backend
        .backup_import(&text, DuplicatePolicy::Skip)
        .await
        .unwrap();
    credentials(&new).await;
    let report = new.backend.fetch_missing_keys().await.unwrap();
    assert_eq!(report.fetched, [sim_id(1)]);
    assert_eq!(report.not_found, [sim_id(2)]);
    assert_eq!(new.backend.devices_without_key().unwrap(), [sim_id(2)]);
}

#[tokio::test]
async fn importar_donde_ya_estan_los_focos_no_toca_sus_claves() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    configure(&h).await;
    let text = h.backend.backup_export().unwrap();
    let keys = [key_of(&h, &sim_id(1)), key_of(&h, &sim_id(2))];
    let summary = h.backend.backup_inspect(&text).unwrap();
    assert!(summary.new_devices.is_empty());
    assert_eq!(summary.existing_devices.len(), 2);

    rename_with_fixed_ip(&h, sim_id(1), "Renombrado");
    h.backend
        .set_device_in_effect(sim_id(1), true)
        .await
        .unwrap();

    let skipped = h
        .backend
        .backup_import(&text, DuplicatePolicy::Skip)
        .await
        .unwrap();
    assert!(skipped.devices.is_empty());
    let first = h.backend.devices_list().unwrap().remove(0);
    assert_eq!(first.name, "Renombrado");
    assert!(first.in_effect);

    let replaced = h
        .backend
        .backup_import(&text, DuplicatePolicy::Replace)
        .await
        .unwrap();
    assert_eq!(replaced.devices, [sim_id(1), sim_id(2)]);
    let first = h.backend.devices_list().unwrap().remove(0);
    assert_eq!(first.name, "Mi foco 1");
    assert!(!first.in_effect);
    assert_eq!(first.ip.as_deref(), Some(FIXED_IP));
    assert_eq!([key_of(&h, &sim_id(1)), key_of(&h, &sim_id(2))], keys);
}

/// Importar con el efecto andando: los focos sin clave no entran (ni avisan como si la clave
/// estuviera mal) y, al bajarla, se suman solos.
#[tokio::test]
async fn con_el_efecto_andando_los_focos_importados_entran_cuando_tienen_clave() {
    let h = new_computer(2).await;
    configure(&h).await;
    let b = &h.backend;
    b.set_device_in_effect(sim_id(1), true).await.unwrap();
    let text = b.backup_export().unwrap();
    b.device_remove(sim_id(2)).await.unwrap();
    h.screen.paint([255, 0, 0]);
    b.session_start().await.unwrap();
    eventually(
        "el primer foco sigue la pantalla",
        Duration::from_secs(8),
        || h.sims[0].dp("21") == Some(json!("colour")),
    )
    .await;
    let raised = || {
        h.events_where(|e| match e {
            AppEvent::ErrorRaised(error) => Some(error.code),
            _ => None,
        })
    };
    let before = raised();

    b.backup_import(&text, DuplicatePolicy::Skip).await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(raised(), before, "no avisa errores de los focos sin clave");
    assert_ne!(b.session_state(), SessionState::Idle);

    b.fetch_missing_keys().await.unwrap();
    eventually("el segundo foco entra solo", Duration::from_secs(8), || {
        h.sims[1].dp("21") == Some(json!("colour"))
    })
    .await;
    b.session_stop().await.unwrap();
}

/// Pisar un foco que está en el efecto lo vuelve a tomar con lo nuevo (la calibración y la
/// versión solo se leen al tomarlo).
#[tokio::test]
async fn pisar_un_foco_del_efecto_lo_vuelve_a_tomar() {
    let h = harness(Setup { sims: 2, lan: true }).await;
    configure(&h).await;
    let b = &h.backend;
    b.set_device_in_effect(sim_id(1), true).await.unwrap();
    let text = b.backup_export().unwrap();
    h.screen.paint([255, 0, 0]);
    b.session_start().await.unwrap();
    eventually("siguen la pantalla", Duration::from_secs(8), || {
        h.sims.iter().all(|s| s.dp("21") == Some(json!("colour")))
    })
    .await;
    let starts = || {
        h.events_where(|e| match e {
            AppEvent::DeviceHealth(health)
                if health.device == sim_id(1)
                    && health.state == openlight_types::DeviceSessionState::Connecting =>
            {
                Some(())
            }
            _ => None,
        })
        .len()
    };
    let before = starts();
    b.backup_import(&text, DuplicatePolicy::Replace)
        .await
        .unwrap();
    eventually("se lo vuelve a tomar", Duration::from_secs(8), || {
        starts() > before
            && h.sims[0].dp("21") == Some(json!("colour"))
            && b.session_state() == SessionState::Active
    })
    .await;
    b.session_stop().await.unwrap();
}

#[tokio::test]
async fn un_archivo_que_no_sirve_da_un_error_claro_y_no_cambia_nada() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let b = &h.backend;
    let profiles = b.profiles_list().unwrap();
    let cases = [
        ("{ esto no es json", ErrorCode::BackupInvalid),
        (r#"[{"id": "x", "key": "y"}]"#, ErrorCode::BackupInvalid),
        (
            r#"{"format": "openlight-backup", "version": 2}"#,
            ErrorCode::BackupUnsupportedVersion,
        ),
    ];
    for (content, code) in cases {
        for error in [
            b.backup_inspect(content).unwrap_err(),
            b.backup_import(content, DuplicatePolicy::Replace)
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code, code, "{content}");
            assert_eq!(error.title, code.title_key());
            assert_eq!(error.actions, [FixAction::Retry]);
            if code == ErrorCode::BackupUnsupportedVersion {
                assert_eq!(error.params["found"], "2");
                assert_eq!(error.params["supported"], "1");
            }
        }
    }
    assert!(b.devices_list().unwrap().is_empty());
    assert_eq!(b.profiles_list().unwrap(), profiles);
    assert_eq!(b.settings_get().unwrap().active_profile, None::<ProfileId>);
}

#[tokio::test]
async fn si_no_se_puede_escribir_el_archivo_se_avisa() {
    let h = harness(Setup {
        sims: 0,
        lan: false,
    })
    .await;
    let missing = h.dir.path().join("no-existe").join("respaldo.json");
    let error = h.backend.backup_save(&missing).unwrap_err();
    assert_eq!(error.code, ErrorCode::Storage);
}

/// Cada secreto en las formas en que podría aparecer escrito.
fn forms(value: &str) -> Vec<String> {
    let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
    vec![
        value.to_owned(),
        hex.to_uppercase(),
        hex,
        BASE64.encode(value),
        format!("{:?}", value.as_bytes()),
    ]
}

/// Cualquier cosa con forma de IPv4.
fn ipv4_in(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|part| part.parse::<Ipv4Addr>().is_ok())
        .collect()
}

#[tokio::test]
async fn ninguna_clave_credencial_ni_ip_sale_en_el_respaldo() {
    let h = harness_overriding(
        Setup { sims: 2, lan: true },
        Overrides {
            local_key: Some(*LOCAL_KEY),
            ..Overrides::default()
        },
    )
    .await;
    configure(&h).await;
    rename_with_fixed_ip(&h, sim_id(1), "Mi foco 1");
    let local_key = String::from_utf8_lossy(LOCAL_KEY).into_owned();
    // Secretos reconocibles en todos los lugares donde la app los guarda
    for (key, value) in [
        (SecretKey::DeviceLocalKey(sim_id(2)), SECOND_KEY),
        (SecretKey::CloudAccessId, ACCESS_ID),
        (SecretKey::CloudAccessSecret, ACCESS_SECRET),
    ] {
        h.secrets.set(&key, &value.to_owned().into()).unwrap();
    }
    let store = openlight_core::Store::open(h.backend.data_dir()).unwrap();
    store.set_last_ip(&sim_id(1), LAST_IP).unwrap();

    let path = h.dir.path().join("respaldo.json");
    h.backend.backup_save(&path).unwrap();
    let file = std::fs::read_to_string(&path).unwrap();

    // La prueba tiene con qué fallar: todo eso está guardado en esta compu
    assert_eq!(key_of(&h, &sim_id(1)), Some(local_key.clone()));
    let data = |name: &str| std::fs::read_to_string(h.backend.data_dir().join(name)).unwrap();
    assert!(data("devices.json").contains(FIXED_IP));
    assert!(data("devices.json").contains("127.0.0.1"));
    assert!(data("state.json").contains(LAST_IP));
    assert!(file.contains("Mi foco 2"), "el respaldo está vacío: {file}");

    let mut leaks = Vec::new();
    for (what, secret) in [
        ("clave local", local_key.as_str()),
        ("segunda clave local", SECOND_KEY),
        ("Access ID", ACCESS_ID),
        ("Access Secret", ACCESS_SECRET),
        ("Access ID validado", FAKE_ACCESS_ID),
        ("Access Secret validado", FAKE_ACCESS_SECRET),
    ] {
        if forms(secret)
            .iter()
            .any(|form| file.contains(form.as_str()))
        {
            leaks.push(what.to_owned());
        }
    }
    for ip in ipv4_in(&file) {
        leaks.push(format!("la IP {ip}"));
    }
    // Ningún campo con nombre de secreto o de red, en ningún nivel
    fn names<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => map.iter().for_each(|(k, v)| {
                out.push(k);
                names(v, out);
            }),
            Value::Array(items) => items.iter().for_each(|v| names(v, out)),
            _ => {}
        }
    }
    let value: Value = serde_json::from_str(&file).unwrap();
    let mut fields = Vec::new();
    names(&value, &mut fields);
    for field in fields {
        if ["key", "secret", "token", "access", "region", "mapping"]
            .iter()
            .any(|word| field.contains(word))
            || ["ip", "auto_ip", "last_ip"].contains(&field)
        {
            leaks.push(format!("el campo {field}"));
        }
    }
    assert!(leaks.is_empty(), "en el respaldo salió: {leaks:?}");
}

#[test]
fn los_avisos_del_respaldo_tienen_texto_en_los_dos_idiomas() {
    for lang in ["es", "en"] {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../ui/src/lib/i18n/{lang}.json"));
        let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let key = openlight_core::backup::WARN_ZONES_MOVED;
        let text = key
            .split('.')
            .try_fold(&json, |node, part| node.get(part))
            .and_then(Value::as_str);
        assert!(text.is_some_and(|t| !t.is_empty()), "falta {key} en {lang}");
    }
}
