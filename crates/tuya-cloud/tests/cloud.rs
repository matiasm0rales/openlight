//! Pruebas del cliente contra un servidor falso (`wiremock`) con las fixtures de `fixtures/cloud/`.

use std::path::PathBuf;

use openlight_types::{
    CloudCredentials, CloudError, DeviceId, ErrorCode, Region, Secret, TuyaCloud,
};
use serde_json::Value;
use tuya_cloud::sign::{sign, sign_path, string_to_sign};
use tuya_cloud::{TuyaCloudClient, to_user_facing};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

const ACCESS_ID: &str = "fakeaccessid00000000";
const ACCESS_SECRET: &str = "fakeaccesssecret0000000000000000";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/cloud")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn json(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(fixture(name))
}

fn token_of(name: &str) -> String {
    fixture(name)["result"]["access_token"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn creds(region: Region) -> CloudCredentials {
    CloudCredentials {
        access_id: ACCESS_ID.into(),
        access_secret: Secret::new(ACCESS_SECRET.into()),
        region,
    }
}

fn client(server: &MockServer) -> TuyaCloudClient {
    let uri = server.uri();
    TuyaCloudClient::new(creds(Region::Us))
        .unwrap()
        .with_endpoints(move |_| uri.clone())
}

/// Recalcula la firma de cada pedido que llega, con el mismo algoritmo de tinytuya.
struct ValidSignature;

impl Match for ValidSignature {
    fn matches(&self, request: &Request) -> bool {
        let h = |k: &str| request.headers.get(k).and_then(|v| v.to_str().ok());
        let (Some(client_id), Some(t), Some(got)) = (h("client_id"), h("t"), h("sign")) else {
            return false;
        };
        let query: Vec<(String, String)> = request.url.query_pairs().into_owned().collect();
        let query: Vec<(&str, String)> =
            query.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let to_sign = string_to_sign("GET", b"", &sign_path(request.url.path(), &query));
        client_id == ACCESS_ID
            && h("sign_method") == Some("HMAC-SHA256")
            && sign(ACCESS_ID, ACCESS_SECRET, h("access_token"), t, &to_sign) == got
    }
}

async fn mount_token(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v1.0/token"))
        .and(query_param("grant_type", "1"))
        .and(ValidSignature)
        .respond_with(json("token_ok.json"))
        .mount(server)
        .await;
}

#[test]
fn firma_igual_a_tinytuya() {
    let golden = fixture("firmas.json");
    let secret = golden["access_secret"].as_str().unwrap();
    let id = golden["access_id"].as_str().unwrap();
    for case in golden["cases"].as_array().unwrap() {
        let url = reqwest::Url::parse(case["url"].as_str().unwrap()).unwrap();
        let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let query: Vec<(&str, String)> =
            query.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let to_sign = string_to_sign(
            case["method"].as_str().unwrap(),
            b"",
            &sign_path(url.path(), &query),
        );
        let got = sign(
            id,
            secret,
            case["token"].as_str(),
            case["t"].as_str().unwrap(),
            &to_sign,
        );
        assert_eq!(got, case["sign"].as_str().unwrap(), "caso {}", case["name"]);
    }
}

#[tokio::test]
async fn valida_y_devuelve_el_uid_de_smart_life() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .and(header("access_token", token_of("token_ok.json").as_str()))
        .and(ValidSignature)
        .respond_with(json("devices_page1.json"))
        .mount(&server)
        .await;

    let account = client(&server).validate().await.unwrap();
    let expected = fixture("devices_page1.json")["result"]["devices"][0]["uid"].clone();
    assert_eq!(account.uid, expected.as_str().unwrap());
    assert_eq!(account.region, Region::Us);
}

#[tokio::test]
async fn sin_dispositivos_usa_el_uid_del_token() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .respond_with(json("devices_empty.json"))
        .mount(&server)
        .await;
    let account = client(&server).validate().await.unwrap();
    assert_eq!(
        account.uid,
        fixture("token_ok.json")["result"]["uid"].as_str().unwrap()
    );
}

async fn mount_devices(server: &MockServer) {
    let page1 = fixture("devices_page1.json");
    let row_key = page1["result"]["last_row_key"].as_str().unwrap().to_owned();
    let uid = page1["result"]["devices"][0]["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .and(query_param("size", "50"))
        .and(query_param_is_missing("last_row_key"))
        .and(ValidSignature)
        .respond_with(json("devices_page1.json"))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .and(query_param("size", "50"))
        .and(query_param("last_row_key", row_key.as_str()))
        .and(ValidSignature)
        .respond_with(json("devices_page2.json"))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(path("/v1.3/iot-03/devices"))
        .and(query_param("page_size", "75"))
        .and(query_param("source_type", "tuyaUser"))
        .and(query_param("source_id", uid.as_str()))
        .and(ValidSignature)
        .respond_with(json("devices_by_user.json"))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn lista_paginada_y_completa_claves_faltantes() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_devices(&server).await;

    let devices = client(&server).devices().await.unwrap();
    assert_eq!(devices.len(), 3);

    let page1 = &fixture("devices_page1.json")["result"]["devices"];
    let izquierda = &devices[0];
    assert_eq!(izquierda.id.0, page1[0]["id"].as_str().unwrap());
    assert_eq!(izquierda.name, "Luz izquierda");
    assert_eq!(
        izquierda.local_key.expose(),
        page1[0]["local_key"].as_str().unwrap()
    );
    assert_eq!(izquierda.model.as_deref(), Some("W509Z2"));
    assert!(izquierda.is_light() && izquierda.online);

    let enchufe = &devices[1];
    assert!(!enchufe.is_light());
    assert_eq!(enchufe.model, None);
    assert!(!enchufe.online);

    // La página 2 trae la clave vacía; la completa la lista por usuario (`_update_device_list`).
    let derecha = &devices[2];
    assert_eq!(derecha.name, "Luz derecha");
    let by_user = &fixture("devices_by_user.json")["result"]["list"][0];
    assert_eq!(
        derecha.local_key.expose(),
        by_user["local_key"].as_str().unwrap()
    );
    assert_eq!(derecha.product_id, "rawproductcolor02");
}

#[tokio::test]
async fn renueva_el_token_si_tuya_lo_rechaza() {
    let server = MockServer::start().await;
    let (a, b) = (token_of("token_ok.json"), token_of("token_ok_2.json"));
    Mock::given(path("/v1.0/token"))
        .respond_with(json("token_ok.json"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/token"))
        .respond_with(json("token_ok_2.json"))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .and(header("access_token", a.as_str()))
        .respond_with(json("error_1010.json"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .and(header("access_token", b.as_str()))
        .and(ValidSignature)
        .respond_with(json("devices_empty.json"))
        .expect(1)
        .mount(&server)
        .await;

    assert!(client(&server).devices().await.unwrap().is_empty());
}

#[tokio::test]
async fn token_rechazado_dos_veces_es_token_invalido() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .respond_with(json("error_1011.json"))
        .mount(&server)
        .await;
    let err = client(&server).devices().await.unwrap_err();
    assert_eq!(err, CloudError::InvalidToken);
}

#[tokio::test]
async fn permiso_denegado_sugiere_la_region_correcta() {
    let server = MockServer::start().await;
    // Cada región vive bajo su propio prefijo: /us, /eu-w...
    for region in Region::ALL {
        let p = region.code();
        Mock::given(path(format!("/{p}/v1.0/token")))
            .respond_with(json("token_ok.json"))
            .mount(&server)
            .await;
        let devices = if region == Region::EuWest {
            "devices_page1.json"
        } else {
            "error_1106.json"
        };
        Mock::given(path(format!("/{p}/v1.0/iot-01/associated-users/devices")))
            .respond_with(json(devices))
            .mount(&server)
            .await;
    }
    let uri = server.uri();
    let cloud = TuyaCloudClient::new(creds(Region::Us))
        .unwrap()
        .with_endpoints(move |r| format!("{uri}/{}", r.code()));

    let err = cloud.validate().await.unwrap_err();
    assert_eq!(
        err,
        CloudError::PermissionDenied {
            suggested_region: Some(Region::EuWest)
        }
    );
    let ui = to_user_facing(&err);
    assert_eq!(ui.code, ErrorCode::CloudWrongRegion);
    assert_eq!(ui.params["region"], "eu-w");
}

#[tokio::test]
async fn permiso_denegado_en_todas_las_regiones() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .respond_with(json("error_1106.json"))
        .mount(&server)
        .await;
    let err = client(&server).validate().await.unwrap_err();
    assert_eq!(
        err,
        CloudError::PermissionDenied {
            suggested_region: None
        }
    );
    assert_eq!(to_user_facing(&err).code, ErrorCode::CloudPermissionDenied);
}

#[tokio::test]
async fn errores_de_credenciales_y_de_plan() {
    let casos = [
        ("error_1004.json", true, CloudError::InvalidSignature),
        ("error_1001.json", true, CloudError::InvalidSignature),
        ("error_1005.json", true, CloudError::InvalidToken),
        (
            "error_28841105.json",
            false,
            CloudError::ServiceNotAuthorized,
        ),
        (
            "error_28841101.json",
            false,
            CloudError::ServiceNotAuthorized,
        ),
        ("error_28841002.json", false, CloudError::TrialExpired),
    ];
    for (file, at_token, expected) in casos {
        let server = MockServer::start().await;
        if at_token {
            Mock::given(path("/v1.0/token"))
                .respond_with(json(file))
                .mount(&server)
                .await;
        } else {
            mount_token(&server).await;
            Mock::given(path("/v1.0/iot-01/associated-users/devices"))
                .respond_with(json(file))
                .mount(&server)
                .await;
        }
        let err = client(&server).validate().await.unwrap_err();
        assert_eq!(err, expected, "{file}");
        assert!(!to_user_facing(&err).actions.is_empty());
    }
}

#[tokio::test]
async fn otro_codigo_queda_como_error_de_api() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.0/iot-01/associated-users/devices"))
        .respond_with(json("error_1013.json"))
        .mount(&server)
        .await;
    let err = client(&server).devices().await.unwrap_err();
    assert_eq!(
        err,
        CloudError::Api {
            code: 1013,
            msg: "request time is invalid".into()
        }
    );
}

#[tokio::test]
async fn respuesta_que_no_es_json() {
    let server = MockServer::start().await;
    Mock::given(path("/v1.0/token"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"))
        .mount(&server)
        .await;
    let err = client(&server).validate().await.unwrap_err();
    assert!(matches!(err, CloudError::Decode(_)), "{err:?}");
}

#[tokio::test]
async fn sin_red_es_error_de_red() {
    let cloud = TuyaCloudClient::new(creds(Region::Us))
        .unwrap()
        .with_endpoints(|_| "http://127.0.0.1:9".to_owned());
    let err = cloud.validate().await.unwrap_err();
    assert!(matches!(err, CloudError::Network(_)), "{err:?}");
    assert_eq!(to_user_facing(&err).code, ErrorCode::CloudNetwork);
}

#[tokio::test]
async fn mapeo_como_build_mapping_y_cacheado_por_producto() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    mount_devices(&server).await;
    let page1 = fixture("devices_page1.json");
    let id = page1["result"]["devices"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    Mock::given(path(format!("/v1.1/devices/{id}/specifications")))
        .and(ValidSignature)
        .respond_with(json("specifications_light.json"))
        .expect(1)
        .mount(&server)
        .await;

    let cloud = client(&server);
    cloud.devices().await.unwrap();
    let m = cloud.mapping(&DeviceId(id.clone())).await.unwrap();
    // 8 de `functions` + 3 que solo están en `status` (30, 34 y `remote_name` sin dp_id).
    assert_eq!(m.0.len(), 11);
    assert_eq!(m.0["20"].code, "switch_led");
    assert_eq!(m.0["21"].values["range"][1], "colour");
    assert_eq!(m.0["22"].values["max"], 1000);
    assert_eq!(m.0["24"].kind, "Json");
    assert!(
        m.0["24"]
            .raw_values
            .as_deref()
            .unwrap()
            .starts_with("{\"h\"")
    );
    assert_eq!(m.0["26"].values["unit"], "s");
    assert_eq!(m.0["28"].code, "control_data");
    assert_eq!(
        m.0["remote_name"].values,
        Value::String("{\"maxlen\":32}".into())
    );

    // Segunda vez: sale de la caché (el mock exige un solo pedido).
    assert_eq!(cloud.mapping(&DeviceId(id)).await.unwrap(), m);
}

#[tokio::test]
async fn dispositivo_sin_dps_da_mapeo_vacio() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(path("/v1.1/devices/abc123/specifications"))
        .respond_with(json("specifications_not_supported.json"))
        .mount(&server)
        .await;
    let m = client(&server)
        .mapping(&DeviceId("abc123".into()))
        .await
        .unwrap();
    assert!(m.0.is_empty());
}

#[tokio::test]
async fn id_raro_no_arma_rutas() {
    let server = MockServer::start().await;
    let err = client(&server)
        .mapping(&DeviceId("../v1.0/token".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, CloudError::Decode(_)));
}
