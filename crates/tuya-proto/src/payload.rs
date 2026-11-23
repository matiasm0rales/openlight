//! Armado de payloads JSON como `XenonDevice.generate_payload` (tinytuya 1.20).
//!
//! Reproduce el `payload_dict` fusionado (`default` → `v3.4`/`v3.5` → `device22`), el remapeo de
//! comandos (CONTROL→CONTROL_NEW y DP_QUERY→DP_QUERY_NEW en 3.4+; DP_QUERY→CONTROL_NEW en
//! device22), el orden de las claves y el JSON compacto. El tiempo (`t`) se inyecta.
//!
//! Diferencias deliberadas con tinytuya:
//! - No hay estado: tinytuya hace una copia superficial del diccionario y, si una llamada sin
//!   `data` sigue a otra con `data`, puede arrastrar los `dps` anteriores. Acá cada llamada es
//!   independiente.
//! - No se arman payloads de subdispositivos zigbee (`cid`) ni de gateways.

use openlight_types::ProtocolVersion;
use serde_json::Value;

use crate::consts::cmd;
use crate::pyjson::{Json, dumps_compact};

/// Tipo de dispositivo para elegir el payload (`dev_type` de tinytuya).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DevType {
    /// El normal.
    #[default]
    Default,
    /// Focos que responden `data unvalid` a DP_QUERY: consultan con CONTROL_NEW y la lista de
    /// DPs.
    Device22,
}

/// Comando y payload listos para cifrar y empaquetar (`MessagePayload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePayload {
    /// Comando final (ya remapeado).
    pub cmd: u32,
    /// Payload en claro.
    pub payload: Vec<u8>,
}

impl MessagePayload {
    /// Crea un payload crudo.
    pub fn new(cmd: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            cmd,
            payload: payload.into(),
        }
    }
}

/// DPs en el orden en que se quieren mandar.
pub type OrderedDps = Vec<(String, Value)>;

/// Lo que va en el parámetro `data` de `generate_payload`.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadData {
    /// DPs con orden explícito (`{"20": true, "21": "colour"}`).
    Dps(OrderedDps),
    /// Cualquier otro valor (por ejemplo la lista de UPDATEDPS, `[18, 19, 20]`).
    Value(Value),
}

impl PayloadData {
    /// DPs desde pares clave-valor, en ese orden.
    pub fn dps<K: Into<String>, V: Into<Value>>(items: impl IntoIterator<Item = (K, V)>) -> Self {
        Self::Dps(
            items
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }

    fn to_json(&self) -> Json {
        match self {
            Self::Dps(items) => Json::Object(
                items
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Value(v.clone())))
                    .collect(),
            ),
            Self::Value(v) => Json::Value(v.clone()),
        }
    }
}

impl From<&openlight_types::Dps> for PayloadData {
    fn from(dps: &openlight_types::Dps) -> Self {
        Self::Dps(dps.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

/// Parámetros de [`generate_payload`].
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadRequest<'a> {
    /// Versión del protocolo.
    pub version: ProtocolVersion,
    /// Tipo de dispositivo.
    pub dev_type: DevType,
    /// Id del foco (valor por defecto de `gwId`, `devId` y `uid`).
    pub dev_id: &'a str,
    /// Comando pedido (antes del remapeo).
    pub command: u32,
    /// `data`: DPs o valor.
    pub data: Option<PayloadData>,
    /// Reemplaza `gwId`.
    pub gw_id: Option<&'a str>,
    /// Reemplaza `devId`.
    pub dev_id_override: Option<&'a str>,
    /// Reemplaza `uid`.
    pub uid: Option<&'a str>,
    /// `rawData`: reemplaza el objeto `data` entero si el comando lo tiene.
    pub raw_data: Option<Value>,
    /// `reqType` (LAN_EXT_STREAM).
    pub req_type: Option<&'a str>,
    /// DPs a pedir en DP_QUERY de device22 (`dps_to_request`); van con valor `null`.
    pub dps_to_request: Vec<String>,
    /// Segundos desde la época Unix (`int(time.time())`).
    pub t: u64,
}

impl<'a> PayloadRequest<'a> {
    /// Pedido mínimo: el resto de los campos en su valor por defecto.
    pub fn new(version: ProtocolVersion, dev_id: &'a str, command: u32, t: u64) -> Self {
        Self {
            version,
            dev_type: DevType::Default,
            dev_id,
            command,
            data: None,
            gw_id: None,
            dev_id_override: None,
            uid: None,
            raw_data: None,
            req_type: None,
            dps_to_request: Vec::new(),
            t,
        }
    }

    /// Agrega `data`.
    #[must_use]
    pub fn with_data(mut self, data: PayloadData) -> Self {
        self.data = Some(data);
        self
    }

    /// Cambia el tipo de dispositivo.
    #[must_use]
    pub fn with_dev_type(mut self, dev_type: DevType) -> Self {
        self.dev_type = dev_type;
        self
    }
}

/// Campos de las plantillas de `payload_dict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    GwId,
    DevId,
    Uid,
    /// `"t": ""` → texto.
    TStr,
    /// `"t": "int"` → número.
    TInt,
    /// `"protocol": 5`.
    Protocol5,
    /// `"data": {}`.
    Data,
    /// `"dpId": [18, 19, 20]`.
    DpId,
    /// `"reqType": ""`.
    ReqType,
}

use Field::{Data, DevId, DpId, GwId, Protocol5, ReqType, TInt, TStr, Uid};

/// Comando final y plantilla, tras fusionar `payload_dict` como `generate_payload`.
fn template(version: ProtocolVersion, dev_type: DevType, command: u32) -> (u32, &'static [Field]) {
    const GENERIC: &[Field] = &[GwId, DevId, Uid, TStr];
    if dev_type == DevType::Device22 && command == cmd::DP_QUERY {
        return (cmd::CONTROL_NEW, &[DevId, Uid, TStr]);
    }
    let v34 = matches!(version, ProtocolVersion::V34 | ProtocolVersion::V35);
    match command {
        cmd::CONTROL if v34 => (cmd::CONTROL_NEW, &[Protocol5, TInt, Data]),
        cmd::CONTROL_NEW if v34 => (cmd::CONTROL_NEW, &[Protocol5, TInt, Data]),
        cmd::DP_QUERY if v34 => (cmd::DP_QUERY_NEW, &[]),
        cmd::DP_QUERY_NEW if v34 => (cmd::DP_QUERY_NEW, &[]),
        cmd::AP_CONFIG | cmd::DP_QUERY => (command, GENERIC),
        cmd::CONTROL | cmd::CONTROL_NEW | cmd::DP_QUERY_NEW => (command, &[DevId, Uid, TStr]),
        cmd::STATUS | cmd::HEART_BEAT => (command, &[GwId, DevId]),
        cmd::UPDATEDPS => (command, &[DpId]),
        cmd::LAN_EXT_STREAM => (command, &[ReqType, Data]),
        _ => (command, GENERIC),
    }
}

/// Arma comando y payload como `generate_payload`.
pub fn generate_payload(req: &PayloadRequest<'_>) -> MessagePayload {
    let (command, fields) = template(req.version, req.dev_type, req.command);
    let t = req.t.to_string();
    let mut obj: Vec<(String, Json)> = Vec::with_capacity(fields.len() + 1);
    let mut data_obj: Option<Vec<(String, Json)>> = None;
    for &field in fields {
        let (key, value) = match field {
            GwId => (
                "gwId",
                Json::Value(Value::from(req.gw_id.unwrap_or(req.dev_id))),
            ),
            DevId => (
                "devId",
                Json::Value(Value::from(req.dev_id_override.unwrap_or(req.dev_id))),
            ),
            Uid => (
                "uid",
                Json::Value(Value::from(req.uid.unwrap_or(req.dev_id))),
            ),
            TStr => ("t", Json::Value(Value::from(t.clone()))),
            TInt => ("t", Json::Value(Value::from(req.t))),
            Protocol5 => ("protocol", Json::Value(Value::from(5))),
            Data => {
                data_obj = Some(Vec::new());
                ("data", Json::Object(Vec::new()))
            }
            DpId => ("dpId", Json::Value(serde_json::json!([18, 19, 20]))),
            ReqType => (
                "reqType",
                Json::Value(Value::from(req.req_type.unwrap_or(""))),
            ),
        };
        obj.push((key.to_owned(), value));
    }

    let has = |k: &str| obj.iter().any(|(key, _)| key == k);
    let set = |obj: &mut Vec<(String, Json)>, k: &str, v: Json| {
        if let Some(slot) = obj.iter_mut().find(|(key, _)| key == k) {
            slot.1 = v;
        } else {
            obj.push((k.to_owned(), v));
        }
    };

    if let (Some(raw), true) = (&req.raw_data, data_obj.is_some()) {
        set(&mut obj, "data", Json::Value(raw.clone()));
    } else if let Some(data) = &req.data {
        if has("dpId") {
            set(&mut obj, "dpId", data.to_json());
        } else if let Some(mut inner) = data_obj.take() {
            inner.push(("dps".to_owned(), data.to_json()));
            set(&mut obj, "data", Json::Object(inner));
        } else {
            set(&mut obj, "dps", data.to_json());
        }
    } else if req.dev_type == DevType::Device22 && req.command == cmd::DP_QUERY {
        let dps = req
            .dps_to_request
            .iter()
            .map(|k| (k.clone(), Json::Value(Value::Null)))
            .collect();
        set(&mut obj, "dps", Json::Object(dps));
    }

    MessagePayload {
        cmd: command,
        payload: dumps_compact(&Json::Object(obj)).into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "abc";

    fn texto(req: &PayloadRequest<'_>) -> (u32, String) {
        let p = generate_payload(req);
        (p.cmd, String::from_utf8(p.payload).unwrap())
    }

    #[test]
    fn remapea_los_comandos_en_3_4() {
        for v in [ProtocolVersion::V34, ProtocolVersion::V35] {
            assert_eq!(
                texto(&PayloadRequest::new(v, ID, cmd::DP_QUERY, 5)),
                (cmd::DP_QUERY_NEW, "{}".into())
            );
            let req = PayloadRequest::new(v, ID, cmd::CONTROL, 5)
                .with_data(PayloadData::dps([("1", true)]));
            assert_eq!(
                texto(&req),
                (
                    cmd::CONTROL_NEW,
                    r#"{"protocol":5,"t":5,"data":{"dps":{"1":true}}}"#.into()
                )
            );
        }
        let req = PayloadRequest::new(ProtocolVersion::V33, ID, cmd::CONTROL, 5)
            .with_data(PayloadData::dps([("1", true)]));
        assert_eq!(
            texto(&req),
            (
                cmd::CONTROL,
                r#"{"devId":"abc","uid":"abc","t":"5","dps":{"1":true}}"#.into()
            )
        );
    }

    #[test]
    fn device22_consulta_con_control_new() {
        let mut req = PayloadRequest::new(ProtocolVersion::V33, ID, cmd::DP_QUERY, 5)
            .with_dev_type(DevType::Device22);
        req.dps_to_request = vec!["1".into(), "20".into()];
        assert_eq!(
            texto(&req),
            (
                cmd::CONTROL_NEW,
                r#"{"devId":"abc","uid":"abc","t":"5","dps":{"1":null,"20":null}}"#.into()
            )
        );
    }

    #[test]
    fn otros_comandos_y_overrides() {
        let mut req = PayloadRequest::new(ProtocolVersion::V31, ID, cmd::STATUS, 5);
        req.gw_id = Some("gw");
        assert_eq!(
            texto(&req),
            (cmd::STATUS, r#"{"gwId":"gw","devId":"abc"}"#.into())
        );
        let req = PayloadRequest::new(ProtocolVersion::V33, ID, 99, 5);
        assert_eq!(
            texto(&req).1,
            r#"{"gwId":"abc","devId":"abc","uid":"abc","t":"5"}"#
        );
        let mut req = PayloadRequest::new(ProtocolVersion::V34, ID, cmd::LAN_EXT_STREAM, 5);
        req.req_type = Some("subdev_online_stat_query");
        req.raw_data = Some(json!({"x": 1}));
        assert_eq!(
            texto(&req).1,
            r#"{"reqType":"subdev_online_stat_query","data":{"x":1}}"#
        );
        let req = PayloadRequest::new(ProtocolVersion::V33, ID, cmd::UPDATEDPS, 5)
            .with_data(PayloadData::Value(json!([1, 2])));
        assert_eq!(texto(&req).1, r#"{"dpId":[1,2]}"#);
    }
}
