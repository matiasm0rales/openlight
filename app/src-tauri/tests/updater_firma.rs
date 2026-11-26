//! Q4 (seguridad): el updater exige firma.
//!
//! Corre `tauri-plugin-updater` con la configuración real de `tauri.conf.json` (cambiando solo
//! dónde busca y la clave pública) contra un `latest.json` servido en loopback. Las firmas de
//! `fixtures/updater` salen del mismo firmador que usa el release (`tauri signer sign`) con
//! claves descartables: la privada no existe más, acá solo hay públicas. No se instala nada.
#![allow(missing_docs)]

use serde_json::{Value, json};
use tauri::test::mock_builder;
use tauri_plugin_updater::{Error, UpdaterExt};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// La versión que anuncia el `latest.json` de las pruebas.
const ANNOUNCED: &str = "9.9.9";

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/updater/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .trim()
        .to_owned()
}

fn package() -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/updater/paquete.bin",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn config(file: &str) -> Value {
    let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn la_configuracion_exige_firma_y_https() {
    let updater = &config("tauri.conf.json")["plugins"]["updater"];
    assert!(
        updater["pubkey"].as_str().is_some_and(|k| !k.is_empty()),
        "falta la clave pública"
    );
    assert_eq!(updater["requireSignedVersion"], true);
    let endpoints = updater["endpoints"].as_array().unwrap();
    assert!(!endpoints.is_empty());
    for endpoint in endpoints {
        assert!(
            endpoint
                .as_str()
                .unwrap()
                .starts_with("https://github.com/matiasm0rales/openlight/releases/"),
            "{endpoint}"
        );
    }
    // Ninguna puerta de atrás, ni con el nombre alternativo de cada opción
    for name in updater.as_object().unwrap().keys() {
        assert!(
            ["pubkey", "endpoints", "requireSignedVersion"].contains(&name.as_str()),
            "opción del updater sin revisar: {name}"
        );
    }
    // La configuración extra del release no toca la seguridad del updater
    assert!(config("tauri.updater.conf.json").get("plugins").is_none());
}

/// Busca la actualización que anuncia un `latest.json` con `signature` y la descarga (que es
/// donde se verifica). Con `pubkey = None` se usa la clave de `tauri.conf.json`.
async fn download(
    signature: &str,
    artifact: Vec<u8>,
    pubkey: Option<String>,
) -> Result<Vec<u8>, Error> {
    let server = MockServer::start().await;
    Mock::given(path("/latest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": ANNOUNCED,
            "url": format!("{}/paquete", server.uri()),
            "signature": signature,
        })))
        .mount(&server)
        .await;
    Mock::given(path("/paquete"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(artifact))
        .mount(&server)
        .await;

    let mut context = tauri::generate_context!(test = true);
    let updater = context
        .config_mut()
        .plugins
        .0
        .get_mut("updater")
        .and_then(Value::as_object_mut)
        .expect("plugins.updater");
    updater.insert(
        "endpoints".into(),
        json!([format!("{}/latest.json", server.uri())]),
    );
    if let Some(pubkey) = pubkey {
        updater.insert("pubkey".into(), json!(pubkey));
    }
    let builder = mock_builder().plugin(tauri_plugin_updater::Builder::new().build());
    // El menú nativo solo se puede crear en el hilo principal, y las pruebas no corren ahí
    #[cfg(target_os = "macos")]
    let builder = builder.enable_macos_default_menu(false);
    let app = builder.build(context).unwrap();

    let update = app
        .handle()
        .updater_builder()
        .no_proxy()
        .build()
        .unwrap()
        .check()
        .await
        .expect("se lee el latest.json")
        .expect("hay una versión más nueva");
    assert_eq!(update.version, ANNOUNCED);
    update.download(|_, _| {}, || {}).await
}

async fn with_test_key(signature_file: &str, artifact: Vec<u8>) -> Result<Vec<u8>, Error> {
    download(
        &fixture(signature_file),
        artifact,
        Some(fixture("clave-de-prueba.pub")),
    )
    .await
}

#[tokio::test]
async fn un_paquete_bien_firmado_pasa() {
    // El control: sin esto, los rechazos de abajo podrían deberse a cualquier otra cosa
    assert_eq!(
        with_test_key("valida.sig", package()).await.unwrap(),
        package()
    );
}

#[tokio::test]
async fn un_paquete_cambiado_se_rechaza() {
    let mut tampered = package();
    tampered[0] ^= 1;
    let error = with_test_key("valida.sig", tampered).await.unwrap_err();
    assert!(matches!(error, Error::Minisign(_)), "{error:?}");
}

#[tokio::test]
async fn una_firma_hecha_con_otra_clave_se_rechaza() {
    let error = with_test_key("otra-clave.sig", package())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Minisign(_)), "{error:?}");
}

#[tokio::test]
async fn una_firma_que_no_es_una_firma_se_rechaza() {
    for garbage in ["", "bm8gc295IHVuYSBmaXJtYQ==", "###"] {
        let result = download(garbage, package(), Some(fixture("clave-de-prueba.pub"))).await;
        assert!(result.is_err(), "{garbage:?}");
    }
}

#[tokio::test]
async fn un_latest_json_no_puede_colar_una_version_vieja_bien_firmada() {
    // Anuncia 9.9.9 pero entrega un paquete firmado como 0.0.9 (volver a una versión con fallas)
    let error = with_test_key("version-vieja.sig", package())
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::SignedVersionMismatch { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn una_firma_sin_version_se_rechaza() {
    let error = with_test_key("sin-version.sig", package())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::MissingSignedVersion), "{error:?}");
}

#[tokio::test]
async fn con_la_clave_de_la_app_no_entra_nada_firmado_por_otros() {
    // Vale hoy (la clave de ejemplo no verifica nada) y con la clave real cargada
    let result = download(&fixture("valida.sig"), package(), None).await;
    assert!(result.is_err());
}
