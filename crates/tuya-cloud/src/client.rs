//! Cliente HTTP de la OpenAPI: token, dispositivos, mapeos y regiones.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use openlight_types::{
    CloudAccount, CloudCredentials, CloudDevice, CloudError, DeviceId, DpMapping, Region, Secret,
    TuyaCloud,
};
use reqwest::Url;
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::error::{NOT_SUPPORTED_DEVICE, classify, is_token_error};
use crate::mapping::mapping_from_specifications;
use crate::sign::{sign, sign_path, string_to_sign};

/// Todos los dispositivos de los usuarios vinculados al proyecto (paginado con `last_row_key`).
/// Doc de Tuya citada en `Cloud.py::_get_all_devices`: <https://developer.tuya.com/en/docs/cloud/fc19523d18?id=Kakr4p8nq5xsc>.
pub const PATH_ASSOCIATED_DEVICES: &str = "/v1.0/iot-01/associated-users/devices";
/// Dispositivos de un usuario (segunda pasada de tinytuya, "to make sure we have the local key").
/// Doc: <https://developer.tuya.com/en/docs/cloud/dc413408fe?id=Kc09y2ons2i3b>.
pub const PATH_USER_DEVICES: &str = "/v1.3/iot-03/devices";
/// Token en modo simple.
pub const PATH_TOKEN: &str = "/v1.0/token";

/// Tamaños de página que usa tinytuya en cada endpoint.
const PAGE_SIZE_ASSOCIATED: &str = "50";
const PAGE_SIZE_USER: &str = "75";
/// Tope de páginas por si la API devuelve `has_more: true` para siempre.
const MAX_PAGES: usize = 200;
/// Margen para renovar el token antes de que venza.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);
/// Timeout de cada pedido.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Lo que reemplaza a un secreto en un mensaje de error.
const HIDDEN: &str = "[oculto]";

type EndpointFn = dyn Fn(Region) -> String + Send + Sync;

struct Token {
    access_token: Secret<String>,
    expires_at: Instant,
}

#[derive(Default)]
struct State {
    token: Option<Token>,
    /// device id → product id, de la última lista de dispositivos.
    product_of: HashMap<String, String>,
    /// product id → mapeo (tinytuya cachea por producto: mismos DPs para el mismo producto).
    mappings: HashMap<String, DpMapping>,
}

/// Cliente de la Tuya OpenAPI. Implementa [`TuyaCloud`].
///
/// ```no_run
/// # async fn demo(creds: openlight_types::CloudCredentials) -> Result<(), openlight_types::CloudError> {
/// use openlight_types::TuyaCloud;
/// let cloud = tuya_cloud::TuyaCloudClient::new(creds)?;
/// let account = cloud.validate().await?;
/// let devices = cloud.devices().await?;
/// # Ok(()) }
/// ```
pub struct TuyaCloudClient {
    http: reqwest::Client,
    access_id: String,
    access_secret: Secret<String>,
    region: Region,
    endpoint: Arc<EndpointFn>,
    state: Mutex<State>,
}

impl std::fmt::Debug for TuyaCloudClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Sin credenciales: ni siquiera el Access ID tiene que terminar en un log
        f.debug_struct("TuyaCloudClient")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl TuyaCloudClient {
    /// Cliente para las credenciales dadas, contra la URL oficial de su región.
    pub fn new(creds: CloudCredentials) -> Result<Self, CloudError> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            // La OpenAPI no redirige, y seguir una redirección se llevaría la firma y el token
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| CloudError::Network(e.to_string()))?;
        Ok(Self {
            http,
            access_id: creds.access_id,
            access_secret: creds.access_secret,
            region: creds.region,
            endpoint: Arc::new(|r: Region| r.base_url().to_owned()),
            state: Mutex::new(State::default()),
        })
    }

    /// Cambia cómo se calcula la URL base de cada región. Para pruebas (`wiremock`): salvo
    /// contra la propia máquina, los pedidos salen solo por HTTPS.
    pub fn with_endpoints(
        mut self,
        endpoint: impl Fn(Region) -> String + Send + Sync + 'static,
    ) -> Self {
        self.endpoint = Arc::new(endpoint);
        self
    }

    /// Región configurada.
    pub fn region(&self) -> Region {
        self.region
    }

    /// Otro cliente con las mismas credenciales en otra región (sin token ni cachés).
    fn for_region(&self, region: Region) -> Self {
        Self {
            http: self.http.clone(),
            access_id: self.access_id.clone(),
            access_secret: self.access_secret.clone(),
            region,
            endpoint: self.endpoint.clone(),
            state: Mutex::new(State::default()),
        }
    }

    /// Si el error es "permiso denegado", prueba las otras regiones y sugiere la que funcione.
    async fn hint_region(&self, error: CloudError) -> CloudError {
        if !matches!(error, CloudError::PermissionDenied { .. }) {
            return error;
        }
        for region in Region::ALL.into_iter().filter(|r| *r != self.region) {
            if self.for_region(region).account().await.is_ok() {
                return CloudError::PermissionDenied {
                    suggested_region: Some(region),
                };
            }
        }
        CloudError::PermissionDenied {
            suggested_region: None,
        }
    }

    /// Token + primera página de dispositivos, para saber el uid del usuario vinculado.
    async fn account(&self) -> Result<CloudAccount, CloudError> {
        let token_uid = self.ensure_token().await?;
        let first = self
            .get(
                PATH_ASSOCIATED_DEVICES,
                &[("size", PAGE_SIZE_ASSOCIATED.to_owned())],
            )
            .await?;
        let device_uid = first
            .get("devices")
            .and_then(Value::as_array)
            .and_then(|list| list.iter().find_map(|d| d.get("uid")?.as_str()))
            .map(str::to_owned);
        Ok(CloudAccount {
            // Sin dispositivos no hay uid de Smart Life a la vista: queda el del token.
            uid: device_uid.or(token_uid).unwrap_or_default(),
            region: self.region,
        })
    }

    /// Devuelve el uid que trae el token, si se pidió uno nuevo.
    async fn ensure_token(&self) -> Result<Option<String>, CloudError> {
        {
            let state = self.state.lock().await;
            if let Some(token) = &state.token
                && Instant::now() < token.expires_at
            {
                return Ok(None);
            }
        }
        self.refresh_token().await
    }

    async fn refresh_token(&self) -> Result<Option<String>, CloudError> {
        self.state.lock().await.token = None;
        let result = self
            .send(PATH_TOKEN, &[("grant_type", "1".to_owned())], None)
            .await
            .map_err(|e| match e {
                // Sin token no hay "token rechazado": se lo trata como cualquier error de token.
                TokenRejected => CloudError::InvalidToken,
                Other(e) => e,
            })?;
        let access_token = result
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| CloudError::Decode("token sin access_token".into()))?
            .to_owned();
        let expire_secs = result
            .get("expire_time")
            .and_then(Value::as_u64)
            .unwrap_or(7200);
        let lifetime = Duration::from_secs(expire_secs).saturating_sub(TOKEN_MARGIN);
        let uid = result.get("uid").and_then(Value::as_str).map(str::to_owned);
        self.state.lock().await.token = Some(Token {
            access_token: Secret::new(access_token),
            expires_at: Instant::now() + lifetime,
        });
        Ok(uid)
    }

    /// GET autenticado. Si Tuya dice que el token no sirve, lo renueva y reintenta una vez
    /// (como `_tuyaplatform(..., recursive=True)`).
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, CloudError> {
        self.ensure_token().await?;
        let token = self.current_token().await;
        match self.send(path, query, token.as_ref()).await {
            Err(TokenRejected) => {
                self.refresh_token().await?;
                let token = self.current_token().await;
                self.send(path, query, token.as_ref())
                    .await
                    .map_err(|e| match e {
                        TokenRejected => CloudError::InvalidToken,
                        Other(e) => e,
                    })
            }
            Err(Other(e)) => Err(e),
            Ok(v) => Ok(v),
        }
    }

    async fn current_token(&self) -> Option<Secret<String>> {
        self.state
            .lock()
            .await
            .token
            .as_ref()
            .map(|t| t.access_token.clone())
    }

    /// Manda un GET firmado y devuelve `result`. Sin token, es el pedido de token.
    async fn send(
        &self,
        path: &str,
        query: &[(&str, String)],
        token: Option<&Secret<String>>,
    ) -> Result<Value, SendError> {
        let base = (self.endpoint)(self.region);
        let mut url = Url::parse(&format!("{}{path}", base.trim_end_matches('/')))
            .map_err(|e| Other(CloudError::Decode(format!("URL inválida: {e}"))))?;
        if url.scheme() != "https" && !is_loopback(&url) {
            return Err(Other(CloudError::Network(
                "la API de Tuya solo se usa por HTTPS".into(),
            )));
        }
        if !query.is_empty() {
            let mut sorted: Vec<&(&str, String)> = query.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let mut pairs = url.query_pairs_mut();
            for (k, v) in sorted {
                pairs.append_pair(k, v);
            }
        }
        let t = now_millis().to_string();
        let token_str = token.map(|t| t.expose().as_str());
        let to_sign = string_to_sign("GET", b"", &sign_path(path, query));
        let signature = sign(
            &self.access_id,
            self.access_secret.expose(),
            token_str,
            &t,
            &to_sign,
        );

        let mut request = self
            .http
            .get(url)
            .header("client_id", &self.access_id)
            .header("sign", &signature)
            .header("t", &t)
            .header("sign_method", "HMAC-SHA256");
        if let Some(token) = token_str {
            request = request.header("access_token", token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| Other(CloudError::Network(e.without_url().to_string())))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| Other(CloudError::Network(e.without_url().to_string())))?;
        let envelope: Value = serde_json::from_slice(&body).map_err(|_| {
            Other(CloudError::Decode(format!(
                "HTTP {status}: la respuesta no es JSON"
            )))
        })?;

        if envelope.get("success").and_then(Value::as_bool) == Some(true) {
            return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
        }
        let code = envelope.get("code").and_then(Value::as_i64).unwrap_or(-1);
        let msg = envelope
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if token.is_some() && is_token_error(code, msg) {
            return Err(TokenRejected);
        }
        // El mensaje llega al log, al diagnóstico y a la UI: si el servidor repite algo de lo
        // que se le mandó, se tapa antes de que salga de acá.
        let msg = scrub(
            msg,
            &[
                &self.access_id,
                self.access_secret.expose(),
                token_str.unwrap_or_default(),
                &signature,
            ],
        );
        Err(Other(classify(code, &msg)))
    }

    /// Recorre un endpoint paginado con `last_row_key` y junta la lista `list_key`.
    async fn paginate(
        &self,
        path: &str,
        mut query: Vec<(&'static str, String)>,
        list_key: &str,
    ) -> Result<Vec<Value>, CloudError> {
        let mut out = Vec::new();
        let mut last_key: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let result = self.get(path, &query).await?;
            if let Some(list) = result.get(list_key).and_then(Value::as_array) {
                out.extend(list.iter().cloned());
            }
            let has_more = result
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let next = result
                .get("last_row_key")
                .and_then(Value::as_str)
                .filter(|k| !k.is_empty())
                .map(str::to_owned);
            // Sin clave nueva no hay cómo pedir la página siguiente: se corta para no repetir.
            if !has_more || next.is_none() || next == last_key {
                break;
            }
            query.retain(|(k, _)| *k != "last_row_key");
            query.push(("last_row_key", next.clone().unwrap_or_default()));
            last_key = next;
        }
        Ok(out)
    }

    /// Lista cruda como `Cloud.py::getdevices` sin `apiDeviceID`: todos los dispositivos
    /// asociados y, por cada uid, la lista del usuario para completar claves que falten.
    async fn raw_devices(&self) -> Result<Vec<Value>, CloudError> {
        let mut devices = self
            .paginate(
                PATH_ASSOCIATED_DEVICES,
                vec![("size", PAGE_SIZE_ASSOCIATED.to_owned())],
                "devices",
            )
            .await?;
        let mut uids = Vec::new();
        let mut seen = HashSet::new();
        for d in &devices {
            if let Some(uid) = d.get("uid").and_then(Value::as_str)
                && seen.insert(uid.to_owned())
            {
                uids.push(uid.to_owned());
            }
        }
        for uid in uids {
            let query = vec![
                ("page_size", PAGE_SIZE_USER.to_owned()),
                ("source_type", "tuyaUser".to_owned()),
                ("source_id", uid),
            ];
            // tinytuya ignora los errores de esta segunda pasada: la lista ya está completa,
            // solo se buscan claves que falten.
            if let Ok(extra) = self.paginate(PATH_USER_DEVICES, query, "list").await {
                merge_device_lists(&mut devices, extra);
            }
        }
        Ok(devices)
    }

    async fn fetch_mapping(&self, device: &DeviceId) -> Result<DpMapping, CloudError> {
        if !is_safe_id(&device.0) {
            return Err(CloudError::Decode(format!(
                "id de dispositivo inválido: {}",
                device.0
            )));
        }
        let product = self.state.lock().await.product_of.get(&device.0).cloned();
        if let Some(product) = &product
            && let Some(cached) = self.state.lock().await.mappings.get(product)
        {
            return Ok(cached.clone());
        }
        let path = format!("/v1.1/devices/{}/specifications", device.0);
        let mapping = match self.get(&path, &[]).await {
            Ok(result) => mapping_from_specifications(&result),
            // "this device does not have any DPs!"
            Err(CloudError::Api { code, ref msg })
                if code == NOT_SUPPORTED_DEVICE || msg == "not support this device" =>
            {
                DpMapping::default()
            }
            Err(e) => return Err(e),
        };
        if let Some(product) = product {
            self.state
                .lock()
                .await
                .mappings
                .insert(product, mapping.clone());
        }
        Ok(mapping)
    }
}

#[async_trait]
impl TuyaCloud for TuyaCloudClient {
    async fn validate(&self) -> Result<CloudAccount, CloudError> {
        match self.account().await {
            Err(e) => Err(self.hint_region(e).await),
            ok => ok,
        }
    }

    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError> {
        let raw = match self.raw_devices().await {
            Ok(raw) => raw,
            Err(e) => return Err(self.hint_region(e).await),
        };
        let devices: Vec<CloudDevice> = raw.iter().filter_map(to_cloud_device).collect();
        let mut state = self.state.lock().await;
        for d in &devices {
            state
                .product_of
                .insert(d.id.0.clone(), d.product_id.clone());
        }
        Ok(devices)
    }

    async fn mapping(&self, device: &DeviceId) -> Result<DpMapping, CloudError> {
        self.fetch_mapping(device).await
    }
}

/// Resultado interno de un pedido: el token rechazado se distingue para reintentar.
enum SendError {
    TokenRejected,
    Other(CloudError),
}
use SendError::{Other, TokenRejected};

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

/// Si la URL apunta a la propia máquina (la única excepción a HTTPS, para el servidor falso de
/// las pruebas: nada sale a la red).
fn is_loopback(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "localhost"
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// `msg` sin ninguno de los `secrets`, sin importar mayúsculas (la firma viaja en mayúsculas).
fn scrub(msg: &str, secrets: &[&str]) -> String {
    let mut out = msg.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        let needle = secret.to_ascii_lowercase();
        // Se sigue después de cada reemplazo: un "secreto" mal escrito puede estar dentro de
        // `HIDDEN` y no se terminaría nunca
        let mut from = 0;
        // `to_ascii_lowercase` no cambia los largos: los índices valen para los dos textos
        while let Some(found) = out[from..].to_ascii_lowercase().find(&needle) {
            let at = from + found;
            out.replace_range(at..at + needle.len(), HIDDEN);
            from = at + HIDDEN.len();
        }
    }
    out
}

/// Los ids de Tuya son alfanuméricos; esto evita armar rutas raras con un id manipulado.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Falsedad de Python (`not valor`), para replicar `_update_device_list`.
fn is_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

/// Port de `Cloud.py::_update_device_list`: agrega los dispositivos nuevos y, en los que ya
/// están, completa las claves que faltan o están vacías.
pub fn merge_device_lists(existing: &mut Vec<Value>, new: Vec<Value>) {
    for new_device in new {
        let Some(new_id) = new_device.get("id").filter(|id| !is_falsy(id)).cloned() else {
            continue;
        };
        let found = existing.iter_mut().find(|d| d.get("id") == Some(&new_id));
        match (found, new_device) {
            (Some(Value::Object(old)), Value::Object(fresh)) => {
                for (k, v) in fresh {
                    if old.get(&k).is_none_or(is_falsy) {
                        old.insert(k, v);
                    }
                }
            }
            (Some(_), _) => {}
            (None, fresh) => existing.push(fresh),
        }
    }
}

fn str_field<'a>(d: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    d.get(key).and_then(Value::as_str)
}

/// Un dispositivo de la API como [`CloudDevice`]. Sin `id` se descarta.
pub fn to_cloud_device(raw: &Value) -> Option<CloudDevice> {
    let d = raw.as_object()?;
    let id = str_field(d, "id").filter(|s| !s.is_empty())?;
    let online = d
        .get("online")
        .or_else(|| d.get("is_online"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(CloudDevice {
        id: DeviceId(id.to_owned()),
        // `filter_devices`: `i['name'].strip()`.
        name: str_field(d, "name").unwrap_or_default().trim().to_owned(),
        local_key: Secret::new(str_field(d, "local_key").unwrap_or_default().to_owned()),
        product_id: str_field(d, "product_id").unwrap_or_default().to_owned(),
        category: str_field(d, "category").unwrap_or_default().to_owned(),
        model: str_field(d, "model")
            .filter(|m| !m.is_empty())
            .map(str::to_owned),
        online,
        cloud_ip: str_field(d, "ip")
            .filter(|ip| !ip.is_empty())
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merge_completa_vacios_y_agrega_nuevos() {
        let mut a = vec![json!({"id": "1", "local_key": "", "name": "A"})];
        merge_device_lists(
            &mut a,
            vec![
                json!({"id": "1", "local_key": "k", "name": "B", "gateway_id": ""}),
                json!({"id": "2", "name": "C"}),
                json!({"name": "sin id"}),
            ],
        );
        assert_eq!(a.len(), 2);
        assert_eq!(a[0]["local_key"], "k");
        assert_eq!(a[0]["name"], "A");
        assert_eq!(a[0]["gateway_id"], "");
        assert_eq!(a[1]["id"], "2");
    }

    #[test]
    fn dispositivo_desde_json() {
        let d = to_cloud_device(&json!({
            "id": "abc", "name": " Luz ", "local_key": "k", "product_id": "p",
            "category": "dj", "model": "", "is_online": true, "ip": "203.0.113.1"
        }))
        .unwrap();
        assert_eq!(d.name, "Luz");
        assert_eq!(d.model, None);
        assert!(d.online);
        assert!(d.is_light());
        assert!(to_cloud_device(&json!({"name": "x"})).is_none());
    }

    #[test]
    fn solo_la_propia_maquina_se_puede_usar_sin_https() {
        let url = |s: &str| Url::parse(s).unwrap();
        for local in [
            "http://127.0.0.1:80/",
            "http://localhost/",
            "http://[::1]:9/",
        ] {
            assert!(is_loopback(&url(local)), "{local}");
        }
        for remote in [
            "http://openapi.tuyaus.com/",
            "http://192.168.1.10/",
            "http://localhost.example.com/",
        ] {
            assert!(!is_loopback(&url(remote)), "{remote}");
        }
    }

    #[test]
    fn tapa_los_secretos_sin_importar_mayusculas() {
        assert_eq!(
            scrub("id=AbC sign=ff00 y otra vez FF00", &["abc", "", "FF00"]),
            "id=[oculto] sign=[oculto] y otra vez [oculto]"
        );
    }

    #[test]
    fn tapar_termina_aunque_el_secreto_este_dentro_del_reemplazo() {
        // Un Access ID mal escrito puede ser cualquier cosa, también un pedazo de "[oculto]"
        assert_eq!(scrub("no, Oso", &["o"]), "n[oculto], [oculto]s[oculto]");
    }

    #[test]
    fn ids_seguros() {
        assert!(is_safe_id("bf0123abc_x-9"));
        assert!(!is_safe_id("../token"));
        assert!(!is_safe_id(""));
    }
}
