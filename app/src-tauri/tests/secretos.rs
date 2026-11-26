//! Q4 (seguridad): ningún secreto sale de donde tiene que estar.
//!
//! Recorre el flujo completo con el backend de verdad, **el cliente real de la nube** (contra una
//! OpenAPI falsa con `wiremock`) y un foco `tuya-sim` con una clave falsa, con el log al máximo
//! detalle (`trace`, mucho más que en la app). Incluye los caminos de error, que es donde un
//! valor suele colarse en un mensaje: credenciales equivocadas, una respuesta de la nube que
//! repite lo que se le mandó, una clave local vieja y un `devices.json` del script mal formado.
//!
//! Después busca la clave local, el Access ID, el Access Secret, los tokens y cada `sign` (en
//! claro, en hexadecimal y en base64) en todo lo que salió: el log (anillo y archivo), cada
//! entrada del zip de diagnóstico, todos los archivos de la carpeta de datos y todo lo que cruza
//! el IPC (respuestas, errores y eventos).
//!
//! Nunca toca el llavero real ni la red de verdad.
#![allow(missing_docs)]

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{Overrides, Setup, eventually, harness_overriding, sim_id};
use openlight_lib::events::AppEvent;
use openlight_lib::logging::{LogHandle, LogRing, RING_CAPACITY, RingLayer};
use openlight_lib::state::CloudFactory;
use openlight_types::ipc::{
    CloudCredentialsInput, DeviceAddRequest, DuplicatePolicy, ManualSet, OnboardingStep,
    SettingScope, TestPattern,
};
use openlight_types::{
    CloudCredentials, Region, SettingId, SettingValue, TuyaCloud, Zone, ZonePreset,
};
use secrets::{SecretKey, SecretStore};
use serde::Serialize;
use serde_json::{Value, json};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tuya_cloud::TuyaCloudClient;
use tuya_cloud::sign::{sign, sign_path, string_to_sign};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const LOCAL_KEY: &[u8; 16] = b"zLk!9fUgA2qW7xPm";
const OLD_LOCAL_KEY: &str = "oLd#4vIeJa8rTy1c";
const ACCESS_ID: &str = "fugaid7Qm2Zk9XwR4tLp";
const ACCESS_SECRET: &str = "fugasecretb81Lk0aZ9QmN3vTy7Wc5Hd";
const WRONG_SECRET: &str = "secretoequivocado0123456789abcde";
const TOKEN: &str = "fugatokenXy33Pq81Rt6Lm0Zc9Vb2Nd4";
const REFRESH_TOKEN: &str = "fugarefreshK2j8Hd5Qw1Er7Ty3Ui9Op";
const UID: &str = "az1111111111111111";

/// La OpenAPI falsa. Firma de verdad: solo da el token si el pedido está firmado con el Access
/// Secret correcto.
struct FakeTuya {
    /// Cuántas de las próximas lecturas del mapeo fallan con un mensaje que repite lo que se
    /// les mandó.
    echo_pending: Arc<Mutex<u32>>,
}

fn header<'a>(request: &'a Request, name: &str) -> &'a str {
    request
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

fn signed_with(request: &Request, secret: &str) -> bool {
    let query: Vec<(String, String)> = request.url.query_pairs().into_owned().collect();
    let query: Vec<(&str, String)> = query.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let to_sign = string_to_sign("GET", b"", &sign_path(request.url.path(), &query));
    let token = Some(header(request, "access_token")).filter(|t| !t.is_empty());
    sign(
        header(request, "client_id"),
        secret,
        token,
        header(request, "t"),
        &to_sign,
    ) == header(request, "sign")
}

fn ok(result: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"success": true, "t": 1, "result": result}))
}

fn fail(code: i64, msg: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"success": false, "t": 1, "code": code, "msg": msg}))
}

impl Respond for FakeTuya {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        if !signed_with(request, ACCESS_SECRET) {
            // Como el peor caso de una API: el error repite la firma y el id que recibió
            return fail(
                1004,
                format!(
                    "sign invalid: client_id={} sign={}",
                    header(request, "client_id"),
                    header(request, "sign")
                ),
            );
        }
        let device = json!({
            "id": sim_id(1).0,
            "name": "Foco falso",
            "local_key": String::from_utf8_lossy(LOCAL_KEY),
            "product_id": "fakeproduct",
            "category": "dj",
            "online": true,
            "uid": UID,
        });
        match request.url.path() {
            "/v1.0/token" => ok(json!({
                "access_token": TOKEN,
                "refresh_token": REFRESH_TOKEN,
                "expire_time": 7200,
                "uid": UID,
            })),
            "/v1.0/iot-01/associated-users/devices" => {
                ok(json!({"devices": [device], "has_more": false}))
            }
            "/v1.3/iot-03/devices" => ok(json!({"list": [device], "has_more": false})),
            _ => {
                let mut pending = self.echo_pending.lock().unwrap();
                if *pending > 0 {
                    *pending -= 1;
                    return fail(
                        99999,
                        format!(
                            "unexpected request: client_id={} access_token={} sign={}",
                            header(request, "client_id"),
                            header(request, "access_token"),
                            header(request, "sign")
                        ),
                    );
                }
                ok(json!({
                    "category": "dj",
                    "functions": [],
                    "status": [
                        {"dp_id": 20, "code": "switch_led", "type": "Boolean", "values": "{}"},
                        {"dp_id": 21, "code": "work_mode", "type": "Enum",
                         "values": "{\"range\":[\"white\",\"colour\"]}"},
                        {"dp_id": 24, "code": "colour_data_v2", "type": "Json", "values": "{}"},
                    ],
                }))
            }
        }
    }
}

/// Todo lo que salió hacia la UI, para revisarlo al final.
#[derive(Default)]
struct Outputs(Vec<(String, String)>);

impl Outputs {
    /// Guarda la respuesta (buena o mala) de un comando tal como cruza el IPC.
    fn command<T: Serialize, E: Serialize>(&mut self, name: &str, result: &Result<T, E>) {
        let text = match result {
            Ok(value) => serde_json::to_string(value),
            Err(error) => serde_json::to_string(error),
        };
        self.0.push((format!("comando {name}"), text.unwrap()));
    }

    fn add(&mut self, place: impl Into<String>, text: impl Into<String>) {
        self.0.push((place.into(), text.into()));
    }
}

fn event_json(event: &AppEvent) -> String {
    match event {
        AppEvent::SessionState(p) => serde_json::to_string(p),
        AppEvent::DeviceHealth(p) => serde_json::to_string(p),
        AppEvent::ColorLive(p) => serde_json::to_string(p),
        AppEvent::PipelineStats(p) => serde_json::to_string(p),
        AppEvent::ErrorRaised(p) => serde_json::to_string(p),
        AppEvent::DiscoveryFound(p) => serde_json::to_string(p),
        AppEvent::EffectDevices(p) => serde_json::to_string(p),
    }
    .unwrap()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Cada secreto en las formas en que podría aparecer escrito.
fn needles(secrets: &[(&str, String)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, value) in secrets {
        let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
        // Los bytes como los imprime el `Debug` de un arreglo (`[122, 76, ...]`)
        let array = format!("{:?}", value.as_bytes());
        for (form, needle) in [
            ("en claro", value.clone()),
            ("en hexadecimal", hex.clone()),
            ("en hexadecimal mayúscula", hex.to_uppercase()),
            ("en base64", BASE64.encode(value)),
            ("como arreglo de bytes", array),
        ] {
            out.push((format!("{name} {form}"), needle));
        }
    }
    out
}

#[tokio::test]
async fn ningun_secreto_sale_en_el_log_el_zip_los_datos_ni_el_ipc() {
    let tmp = tempfile::tempdir().unwrap();
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let log_path = log_dir.join("openlight.prueba.log");
    let file = std::fs::File::create(&log_path).unwrap();
    // Más grande que el de la app, para que no se caiga ninguna línea del principio
    let ring = LogRing::new(200_000);
    tracing_subscriber::registry()
        // `wiremock` es el servidor falso: lo que él anote no es de la app
        .with(EnvFilter::new("trace,wiremock=off"))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(Mutex::new(file))
                .with_ansi(false),
        )
        .with(RingLayer(ring.clone()))
        .try_init()
        .unwrap();

    let server = MockServer::start().await;
    let echo_pending = Arc::new(Mutex::new(1));
    Mock::given(method("GET"))
        .and(path_regex("^/v1"))
        .respond_with(FakeTuya {
            echo_pending: echo_pending.clone(),
        })
        .mount(&server)
        .await;
    let url = server.uri();
    let cloud: CloudFactory = Arc::new(move |creds: CloudCredentials| {
        let url = url.clone();
        TuyaCloudClient::new(creds)
            .map(|c| Arc::new(c.with_endpoints(move |_| url.clone())) as Arc<dyn TuyaCloud>)
    });
    let h = harness_overriding(
        Setup::default(),
        Overrides {
            local_key: Some(*LOCAL_KEY),
            cloud: Some(cloud),
            ..Overrides::default()
        },
    )
    .await;
    let b = &h.backend;
    b.set_logs(LogHandle {
        ring: ring.clone(),
        dir: log_dir.clone(),
        export_dir: tmp.path().join("exports"),
    });
    let id = sim_id(1);
    let sim = &h.sims[0];
    let mut out = Outputs::default();
    let credentials = |secret: &str| CloudCredentialsInput {
        access_id: ACCESS_ID.into(),
        access_secret: secret.into(),
        region: Region::Us,
    };

    // ---------- Nube ----------
    let wrong = b.cloud_validate(credentials(WRONG_SECRET)).await;
    assert!(wrong.is_err());
    out.command("cloud_validate (mal)", &wrong);
    let account = b.cloud_validate(credentials(ACCESS_SECRET)).await;
    assert!(account.is_ok(), "{account:?}");
    out.command("cloud_validate", &account);
    // La primera lectura del mapeo falla repitiendo el token y la firma
    let listed = b.cloud_list_devices().await;
    assert!(listed.is_ok(), "{listed:?}");
    out.command("cloud_list_devices", &listed);

    out.command("discover_lan", &b.discover_lan(1.0).await);

    // ---------- Focos ----------
    let added = b
        .device_add(DeviceAddRequest {
            id: id.clone(),
            name: "Escritorio".into(),
            ip: Some("127.0.0.1".into()),
        })
        .await;
    assert!(added.is_ok(), "{added:?}");
    out.command("device_add", &added);
    out.command("devices_list", &b.devices_list());
    out.command("onboarding_state", &b.onboarding_state());
    out.command(
        "onboarding_save",
        &b.onboarding_save(OnboardingStep::Done, vec![OnboardingStep::Credentials]),
    );
    let tested = b.device_test(id.clone(), TestPattern::RedGreenBlue).await;
    assert!(tested.is_ok(), "{tested:?}");
    out.command("device_test", &tested);
    out.command(
        "device_manual",
        &b.device_manual(id.clone(), ManualSet::Power(true)).await,
    );
    tokio::time::sleep(Duration::from_millis(600)).await;

    // ---------- Sesión, ajustes y calibración ----------
    let displays = b.displays_list().unwrap();
    out.command(
        "display_preview",
        &b.display_preview(displays[0].id.clone()).await,
    );
    out.command(
        "zones_set",
        &b.zones_set(
            id.clone(),
            vec![Zone::from_preset(displays[0].id.clone(), ZonePreset::Left)],
        ),
    );
    out.command(
        "settings_set",
        &b.settings_set(
            SettingScope::Global,
            SettingId::Saturation,
            SettingValue::Number(1.5),
        ),
    );
    out.command("settings_get", &b.settings_get());
    out.command("profiles_list", &b.profiles_list());
    h.screen.paint([255, 0, 255]);
    let started = b.session_start().await;
    assert!(started.is_ok(), "{started:?}");
    out.command("session_start", &started);
    eventually("el foco sigue la pantalla", Duration::from_secs(8), || {
        sim.dp("21") == Some(json!("colour"))
    })
    .await;
    // Con el efecto andando: el foco está ocupado, sale y vuelve a entrar
    let busy = b.device_test(id.clone(), TestPattern::Blink).await;
    assert!(busy.is_err(), "{busy:?}");
    out.command("device_test (en el efecto)", &busy);
    for in_effect in [false, true] {
        let changed = b.set_device_in_effect(id.clone(), in_effect).await;
        assert!(changed.is_ok(), "{changed:?}");
        out.command("device_set_in_effect", &changed);
    }
    out.command("session_stop", &b.session_stop().await);
    let calibrating = b
        .calibration_start(id.clone(), displays[0].id.clone(), true)
        .await;
    assert!(calibrating.is_ok(), "{calibrating:?}");
    out.command("calibration_start", &calibrating);
    out.command(
        "calibration_set",
        &b.calibration_set(1, Some(70.0), Some(1.1)).await,
    );
    out.command("calibration_save", &b.calibration_save().await);

    // ---------- Clave vieja (914) y renovar ----------
    h.secrets
        .set(
            &SecretKey::DeviceLocalKey(id.clone()),
            &OLD_LOCAL_KEY.to_owned().into(),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let rejected = b.device_test(id.clone(), TestPattern::Blink).await;
    assert!(rejected.is_err(), "con la clave vieja tiene que fallar");
    out.command("device_test (clave vieja)", &rejected);
    let renewed = b.renew_key(id.clone()).await;
    assert!(renewed.is_ok(), "{renewed:?}");
    out.command("renew_key", &renewed);

    // ---------- Importar el setup del script ----------
    let broken = tmp.path().join("script-roto");
    std::fs::create_dir_all(&broken).unwrap();
    // Un devices.json que no tiene la forma esperada: el error no puede repetir lo que traía
    std::fs::write(
        broken.join("devices.json"),
        serde_json::to_string(&[String::from_utf8_lossy(LOCAL_KEY)]).unwrap(),
    )
    .unwrap();
    let bad_import = b.import_legacy(broken.to_string_lossy().into_owned()).await;
    assert!(bad_import.is_err());
    out.command("import_legacy (roto)", &bad_import);
    let legacy = tmp.path().join("script");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(
        legacy.join("config.json"),
        json!({"device_id": id.0, "ip": "127.0.0.1", "version": 3.5,
               "local_key": String::from_utf8_lossy(LOCAL_KEY)})
        .to_string(),
    )
    .unwrap();
    let imported = b.import_legacy(legacy.to_string_lossy().into_owned()).await;
    assert!(imported.is_ok(), "{imported:?}");
    out.command("import_legacy", &imported);

    // ---------- La nube falla al agregar ----------
    // Al validar de nuevo se vacía la caché, y el mapeo vuelve a repetir el token y la firma:
    // esta vez el error llega como respuesta del comando (`device_add` lo intenta dos veces)
    b.cloud_validate(credentials(ACCESS_SECRET)).await.unwrap();
    *echo_pending.lock().unwrap() = 2;
    let add_fails = b
        .device_add(DeviceAddRequest {
            id: id.clone(),
            name: "Escritorio".into(),
            ip: None,
        })
        .await;
    assert!(add_fails.is_err(), "{add_fails:?}");
    out.command("device_add (mapeo con error)", &add_fails);

    // ---------- Respaldo ----------
    let content = b.backup_export().unwrap();
    out.add("texto del respaldo", content.clone());
    let backup_file = tmp.path().join("respaldo.json");
    out.command("backup_export", &b.backup_save(&backup_file));
    out.add(
        "archivo del respaldo",
        std::fs::read_to_string(&backup_file).unwrap(),
    );
    out.command("backup_inspect", &b.backup_inspect(&content));
    let restored = b.backup_import(&content, DuplicatePolicy::Replace).await;
    assert!(restored.is_ok(), "{restored:?}");
    out.command("backup_import", &restored);
    // Elegir por error un archivo con la clave: el error no puede repetirla
    let key = String::from_utf8_lossy(LOCAL_KEY);
    for wrong in [
        json!([{"id": id.0, "key": key}]).to_string(),
        json!({"format": "openlight-backup", "version": 1, "created_at": "x",
               "app_version": "x", "devices": key})
        .to_string(),
    ] {
        let rejected = b.backup_inspect(&wrong);
        assert!(rejected.is_err(), "{rejected:?}");
        out.command("backup_inspect (otro archivo)", &rejected);
    }
    h.secrets
        .delete(&SecretKey::DeviceLocalKey(id.clone()))
        .unwrap();
    out.command("devices_without_key", &b.devices_without_key());
    let fetched = b.fetch_missing_keys().await;
    assert!(
        fetched.as_ref().is_ok_and(|r| r.fetched == [id.clone()]),
        "{fetched:?}"
    );
    out.command("fetch_missing_keys", &fetched);

    // ---------- Diagnóstico ----------
    out.command(
        "diagnostics_log",
        &Ok::<_, ()>(b.diagnostics_log(Some(u32::try_from(RING_CAPACITY).unwrap()))),
    );
    let zip_path = b.diagnostics_export(None).await;
    out.command("diagnostics_export", &zip_path);
    let zip_path = zip_path.unwrap();

    // ---------- Qué hay que buscar ----------
    let requests = server.received_requests().await.unwrap_or_default();
    let mut secrets: Vec<(&str, String)> = vec![
        (
            "clave local",
            String::from_utf8_lossy(LOCAL_KEY).into_owned(),
        ),
        ("clave local vieja", OLD_LOCAL_KEY.to_owned()),
        ("Access ID", ACCESS_ID.to_owned()),
        ("Access Secret", ACCESS_SECRET.to_owned()),
        ("Access Secret equivocado", WRONG_SECRET.to_owned()),
        ("token", TOKEN.to_owned()),
        ("refresh token", REFRESH_TOKEN.to_owned()),
    ];
    let signs: Vec<String> = requests
        .iter()
        .map(|r| header(r, "sign").to_owned())
        .collect();
    assert!(!signs.is_empty(), "no se registraron pedidos a la nube");
    secrets.extend(signs.into_iter().map(|s| ("sign", s)));
    let needles = needles(&secrets);

    // ---------- Dónde se busca ----------
    // El log tiene que haber registrado el flujo (si no, la prueba no prueba nada)
    let lines = ring.last(usize::MAX);
    for target in ["tuya_device", "openlight_lib", "tuya_sim"] {
        assert!(
            lines.iter().any(|l| l.target.starts_with(target)),
            "el log no tiene líneas de {target}"
        );
    }
    assert!(
        lines
            .iter()
            .any(|l| l.message.contains("no se pudo leer el mapeo")),
        "no quedó en el log el error de la nube"
    );
    for line in &lines {
        out.add(
            format!("anillo del log ({})", line.target),
            line.message.clone(),
        );
    }
    out.add(
        "archivo de log",
        std::fs::read_to_string(&log_path).unwrap(),
    );

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    for expected in ["health.json", "config.json", "devices.json"] {
        assert!(names.iter().any(|n| n == expected), "falta {expected}");
    }
    for name in &names {
        let mut bytes = Vec::new();
        zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
        out.add(format!("zip: {name}"), String::from_utf8_lossy(&bytes));
    }

    let data = files_under(b.data_dir());
    for expected in [
        "config.json",
        "devices.json",
        "state.json",
        "onboarding.json",
    ] {
        assert!(
            data.iter().any(|p| p.ends_with(expected)),
            "falta {expected} en {data:?}"
        );
    }
    for file in data {
        out.add(
            format!("datos: {}", file.display()),
            String::from_utf8_lossy(&std::fs::read(&file).unwrap()),
        );
    }

    let events = h.events.events();
    assert!(!events.is_empty());
    for event in &events {
        out.add(format!("evento {}", event.name()), event_json(event));
    }

    // ---------- La búsqueda ----------
    let mut leaks: Vec<String> = Vec::new();
    for (place, text) in &out.0 {
        for (what, needle) in &needles {
            if text.contains(needle.as_str()) {
                leaks.push(format!("{what} en {place}"));
            }
        }
    }
    leaks.sort();
    leaks.dedup();
    assert!(
        leaks.is_empty(),
        "secretos filtrados:\n{}",
        leaks.join("\n")
    );
}
