//! Interpretación de los datagramas, sin sockets.
//!
//! El descifrado es de `tuya_proto::udp` (tarea A); acá solo se aplica lo que hace
//! `tinytuya/scanner.py` con el JSON ya descifrado.

use std::net::IpAddr;

use openlight_types::ipc::DiscoveredDevice;
use openlight_types::{DeviceId, ProtocolVersion};
use serde_json::Value;

/// Qué era un datagrama.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Datagram {
    /// El anuncio de un foco.
    Device(DiscoveredDevice),
    /// Otra app pidiendo que los focos se anuncien (`"from": "app"`); tinytuya los ignora.
    App,
    /// Un foco con una versión de protocolo que OpenLight no habla (por ejemplo 3.2).
    UnsupportedVersion {
        /// Id del dispositivo.
        id: String,
        /// Versión anunciada.
        version: String,
    },
    /// No se pudo descifrar, no es JSON o le falta `gwId`.
    Invalid,
}

/// Descifra e interpreta un datagrama recibido desde `src`.
///
/// Como en `scanner.py`: se descartan los de `"from": "app"` y los que no traen `gwId`; el id
/// es `gwId` y la IP es la de origen del paquete (`addr[0]`), no el campo `ip` del JSON.
pub fn parse_datagram(data: &[u8], src: IpAddr) -> Datagram {
    match tuya_proto::udp::decode_broadcast(data) {
        Ok(json) => interpret(&json, src),
        Err(_) => Datagram::Invalid,
    }
}

/// La parte de [`parse_datagram`] que no descifra (útil si el JSON ya está en claro).
pub fn interpret(json: &Value, src: IpAddr) -> Datagram {
    if json.get("from").and_then(Value::as_str) == Some("app") {
        return Datagram::App;
    }
    let Some(id) = json
        .get("gwId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    else {
        return Datagram::Invalid;
    };
    let version_text = match json.get("version") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let Some(version) = ProtocolVersion::parse(&version_text) else {
        return Datagram::UnsupportedVersion {
            id: id.to_owned(),
            version: version_text,
        };
    };
    Datagram::Device(DiscoveredDevice {
        id: DeviceId(id.to_owned()),
        ip: src.to_string(),
        version,
        product_key: json
            .get("productKey")
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty())
            .map(str::to_owned),
    })
}

/// Arma el `REQ_DEVINFO` (0x25) que se manda al puerto 7000 desde `local_ip`, con un IV al azar
/// (`send_discovery_request` usa `os.urandom(12)`).
pub fn req_devinfo(local_ip: IpAddr) -> Vec<u8> {
    tuya_proto::udp::encode_req_devinfo(&local_ip.to_string(), rand::random())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const SRC: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 10));

    #[test]
    fn descarta_apps_y_sin_gwid() {
        assert_eq!(
            interpret(&json!({"from": "app", "ip": "x"}), SRC),
            Datagram::App
        );
        assert_eq!(
            interpret(&json!({"version": "3.3"}), SRC),
            Datagram::Invalid
        );
        assert_eq!(
            interpret(&json!({"gwId": "", "version": "3.3"}), SRC),
            Datagram::Invalid
        );
    }

    #[test]
    fn version_desconocida() {
        assert_eq!(
            interpret(&json!({"gwId": "abc", "version": "3.2"}), SRC),
            Datagram::UnsupportedVersion {
                id: "abc".into(),
                version: "3.2".into()
            }
        );
    }

    #[test]
    fn ip_de_origen_y_sin_product_key() {
        let Datagram::Device(d) = interpret(
            &json!({"gwId": "abc", "version": "3.4", "ip": "10.0.0.9"}),
            SRC,
        ) else {
            panic!("no es un dispositivo");
        };
        assert_eq!(d.ip, "192.0.2.10");
        assert_eq!(d.version, ProtocolVersion::V34);
        assert_eq!(d.product_key, None);
    }

    #[test]
    fn basura_es_invalida() {
        assert_eq!(parse_datagram(b"hola", SRC), Datagram::Invalid);
        assert_eq!(parse_datagram(&[], SRC), Datagram::Invalid);
    }
}
