//! Discovery por UDP: broadcasts de los focos (6666, 6667, 7000) y el pedido `REQ_DEVINFO` de
//! la app, como `tinytuya/core/udp_helper.py` y `scanner.py`. Sin sockets: lo usa
//! `tuya-discovery` (leer) y `tuya-sim` (armar).

use serde_json::Value;

use crate::consts::cmd;
use crate::crypto::{Key, aes_ecb_decrypt, aes_ecb_encrypt, md5};
use crate::error::{ProtoError, Result};
use crate::frame::{Message, Prefix, RetcodeMode, parse_header, unpack_message};
use crate::pyjson::{Json, dumps_spaced};

/// Clave de los broadcasts: `md5(b"yGAdlopoPVldABfn")` (de tuya-convert).
pub const UDP_KEY: Key = [
    0x6c, 0x1e, 0xc8, 0xe2, 0xbb, 0x9b, 0xb5, 0x9a, 0xb5, 0x0b, 0x0d, 0xaf, 0x64, 0x9b, 0x41, 0x0a,
];

/// Calcula [`UDP_KEY`] desde su fuente.
pub fn udp_key() -> Key {
    md5(b"yGAdlopoPVldABfn")
}

fn utf8(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| ProtoError::BadJson("payload no UTF-8".into()))
}

/// Descifra un datagrama como `decrypt_udp` y devuelve el texto (JSON).
///
/// - Sin encabezado válido: el datagrama entero va cifrado con ECB.
/// - 55AA (6666/6667): retcode + JSON en claro, o retcode + ECB(JSON).
/// - 6699 (7000): GCM con la clave UDP, retcode detectado, sin los NUL del final.
///
/// # Errors
/// Errores de descifrado o UTF-8; [`ProtoError::AuthFailed`] si el tag GCM no coincide.
pub fn decrypt_udp(msg: &[u8]) -> Result<String> {
    let Ok(header) = parse_header(msg) else {
        return utf8(aes_ecb_decrypt(&UDP_KEY, msg, false)?);
    };
    match header.prefix {
        Prefix::P55aa => {
            let payload = unpack_message(msg, None, RetcodeMode::Present)?.payload;
            if payload.first() == Some(&b'{')
                && payload.last() == Some(&b'}')
                && let Ok(text) = std::str::from_utf8(&payload)
            {
                return Ok(text.to_owned());
            }
            utf8(aes_ecb_decrypt(&UDP_KEY, &payload, false)?)
        }
        Prefix::P6699 => {
            let frame = unpack_message(msg, Some(&UDP_KEY), RetcodeMode::Detect)?;
            if !frame.crc_good {
                return Err(ProtoError::AuthFailed);
            }
            let text = utf8(frame.payload)?;
            Ok(text.trim_end_matches('\0').to_owned())
        }
    }
}

/// Descifra y parsea un broadcast (`json.loads(decrypt_udp(data))`).
///
/// # Errors
/// Los de [`decrypt_udp`] y [`ProtoError::BadJson`].
pub fn decode_broadcast(msg: &[u8]) -> Result<Value> {
    let text = decrypt_udp(msg)?;
    serde_json::from_str(&text).map_err(|e| ProtoError::BadJson(e.to_string()))
}

/// Arma el pedido `REQ_DEVINFO` que la app manda al puerto 7000 para que los focos 3.5 se
/// anuncien (`send_discovery_request`): `{"from": "app", "ip": <ip>}` con los espacios de
/// `json.dumps`, en un frame 6699 sin retcode, seq 0, cifrado con la clave UDP.
pub fn encode_req_devinfo(ip: &str, iv: [u8; 12]) -> Vec<u8> {
    let body = dumps_spaced(&Json::Object(vec![
        ("from".into(), Value::from("app").into()),
        ("ip".into(), Value::from(ip).into()),
    ]));
    let msg = Message {
        seqno: 0,
        cmd: cmd::REQ_DEVINFO,
        retcode: None,
        payload: body.into_bytes(),
        prefix: Prefix::P6699,
        iv: Some(iv),
    };
    pack_message_infallible(&msg)
}

fn pack_message_infallible(msg: &Message) -> Vec<u8> {
    // Con clave e IV presentes, pack_message no falla.
    crate::frame::pack_message(msg, Some(&UDP_KEY)).unwrap_or_default()
}

/// Tipo de broadcast que manda un foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BroadcastKind {
    /// Puerto 6666: 55AA `UDP_NEW`, retcode 0 + JSON en claro, CRC32.
    Plain6666,
    /// Puerto 6667: 55AA `UDP_NEW`, retcode 0 + ECB(JSON) con la clave UDP, CRC32.
    Encrypted6667,
    /// Puerto 7000: 6699 `BOARDCAST_LPV34`, sin retcode, GCM con la clave UDP.
    Gcm7000,
}

impl BroadcastKind {
    /// Puerto UDP de destino.
    pub const fn port(self) -> u16 {
        match self {
            Self::Plain6666 => crate::consts::UDP_PORT,
            Self::Encrypted6667 => crate::consts::UDP_PORT_ENCRYPTED,
            Self::Gcm7000 => crate::consts::UDP_PORT_APP,
        }
    }
}

/// Arma un broadcast del foco (lado `tuya-sim`), como `fixture_udp` de `gen_proto.py`.
/// `iv` se usa solo en [`BroadcastKind::Gcm7000`] (si falta, se usa uno en cero).
pub fn encode_broadcast(kind: BroadcastKind, json: &[u8], iv: Option<[u8; 12]>) -> Vec<u8> {
    let msg = |cmd, retcode, payload, prefix, iv| Message {
        seqno: 0,
        cmd,
        retcode,
        payload,
        prefix,
        iv,
    };
    match kind {
        BroadcastKind::Plain6666 => crate::frame::pack_message(
            &msg(cmd::UDP_NEW, Some(0), json.to_vec(), Prefix::P55aa, None),
            None,
        )
        .unwrap_or_default(),
        BroadcastKind::Encrypted6667 => {
            let ct = aes_ecb_encrypt(&UDP_KEY, json, true).unwrap_or_default();
            crate::frame::pack_message(&msg(cmd::UDP_NEW, Some(0), ct, Prefix::P55aa, None), None)
                .unwrap_or_default()
        }
        BroadcastKind::Gcm7000 => pack_message_infallible(&msg(
            cmd::BOARDCAST_LPV34,
            None,
            json.to_vec(),
            Prefix::P6699,
            Some(iv.unwrap_or([0; 12])),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_clave_udp_sale_de_su_fuente() {
        assert_eq!(udp_key(), UDP_KEY);
    }

    #[test]
    fn datagrama_sin_encabezado_cifrado_entero() {
        let ct = aes_ecb_encrypt(&UDP_KEY, br#"{"gwId":"x"}"#, true).unwrap();
        assert_eq!(decode_broadcast(&ct).unwrap()["gwId"], "x");
        assert!(decode_broadcast(b"basura").is_err());
    }

    #[test]
    fn el_pedido_de_la_app_se_lee_como_broadcast() {
        let frame = encode_req_devinfo("192.0.2.20", [7; 12]);
        let v = decode_broadcast(&frame).unwrap();
        assert_eq!(v["from"], "app");
        assert_eq!(v["ip"], "192.0.2.20");
    }
}
