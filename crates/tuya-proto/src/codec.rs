//! Cifrado de payloads y armado de frames del lado de la app (`_encode_message`) y descifrado
//! de lo que manda el foco (`_decode_payload`), de `tinytuya/core/XenonDevice.py`.

use openlight_types::ProtocolVersion;
use serde_json::Value;

use crate::consts::{cmd, needs_protocol_header, version_bytes, version_header};
use crate::crypto::{
    Key, aes_ecb_decrypt, aes_ecb_encrypt, base64_decode, base64_encode, md5_signature_v31,
};
use crate::error::{ProtoError, Result};
use crate::frame::{Message, Prefix, pack_message};
use crate::payload::{DevType, MessagePayload};

/// Cifra el payload y arma el frame como `_encode_message`.
///
/// - `key` es la clave vigente: la local_key en 3.1/3.3 y la clave de sesión en 3.4/3.5 una vez
///   negociada (durante la negociación, la local_key).
/// - `iv` es obligatorio en 3.5 (IV de GCM de este frame) y se ignora en el resto.
///
/// | versión | payload |
/// |---|---|
/// | 3.1 | en claro; CONTROL va `"3.1"` + firma MD5 (16) + base64(ECB) |
/// | 3.3 | ECB + PKCS7; los comandos con encabezado van `"3.3"+12×0` + ECB, sin cifrar el encabezado |
/// | 3.4 | ECB(encabezado + payload), HMAC-SHA256 |
/// | 3.5 | frame 6699, GCM(encabezado + payload), sin retcode |
///
/// # Errors
/// [`ProtoError::MissingIv`] en 3.5 sin IV.
pub fn encode_message(
    version: ProtocolVersion,
    key: &Key,
    seqno: u32,
    msg: &MessagePayload,
    iv: Option<[u8; 12]>,
) -> Result<Vec<u8>> {
    let header = version_header(version);
    let mut payload = msg.payload.clone();
    let hmac_key = match version {
        ProtocolVersion::V34 | ProtocolVersion::V35 => {
            if needs_protocol_header(msg.cmd) {
                payload.splice(0..0, header);
            }
            if version == ProtocolVersion::V35 {
                let frame = Message {
                    seqno,
                    cmd: msg.cmd,
                    retcode: None,
                    payload,
                    prefix: Prefix::P6699,
                    iv,
                };
                return pack_message(&frame, Some(key));
            }
            payload = aes_ecb_encrypt(key, &payload, true)?;
            Some(key)
        }
        ProtocolVersion::V33 => {
            payload = aes_ecb_encrypt(key, &payload, true)?;
            if needs_protocol_header(msg.cmd) {
                payload.splice(0..0, header);
            }
            None
        }
        ProtocolVersion::V31 => {
            if msg.cmd == cmd::CONTROL {
                let b64 = base64_encode(&aes_ecb_encrypt(key, &payload, true)?);
                let signature = md5_signature_v31(&b64, key);
                let mut out = Vec::with_capacity(3 + 16 + b64.len());
                out.extend_from_slice(version_bytes(ProtocolVersion::V31));
                out.extend_from_slice(&signature);
                out.extend_from_slice(&b64);
                payload = out;
            }
            None
        }
    };
    let frame = Message {
        seqno,
        cmd: msg.cmd,
        retcode: None,
        payload,
        prefix: Prefix::P55aa,
        iv: None,
    };
    pack_message(&frame, hmac_key)
}

/// Descifra y parsea el payload de un frame del foco como `_decode_payload`.
///
/// Recibe el payload ya sin retcode (y, en 3.5, ya descifrado por
/// [`crate::frame::unpack_message`]). Devuelve `Ok(None)` si el payload está vacío (por
/// ejemplo la respuesta al latido). Como tinytuya, si el JSON trae `data.dps` y no `dps`, copia
/// `data.dps` a `dps`.
///
/// # Errors
/// - [`ProtoError::DataUnvalid`] si el foco respondió `data unvalid` (3.3/3.4): hay que pasar
///   a [`DevType::Device22`] y reintentar.
/// - [`ProtoError::UnexpectedPayload`] si no se puede descifrar o no tiene la forma esperada.
/// - [`ProtoError::BadJson`] si no es JSON.
pub fn decode_payload(
    version: ProtocolVersion,
    key: &Key,
    dev_type: DevType,
    payload: &[u8],
) -> Result<Option<Value>> {
    if payload.is_empty() {
        return Ok(None);
    }
    let mut payload = payload.to_vec();
    if version == ProtocolVersion::V34 {
        // 3.4 cifra también el encabezado de versión.
        payload =
            aes_ecb_decrypt(key, &payload, false).map_err(|_| ProtoError::UnexpectedPayload)?;
    }

    if payload.starts_with(version_bytes(ProtocolVersion::V31)) {
        // "3.1" + 16 de firma MD5 + base64(ECB).
        let b64 = payload.get(3 + 16..).unwrap_or_default();
        payload = aes_ecb_decrypt(key, &base64_decode(b64)?, false)?;
    } else if version != ProtocolVersion::V31 {
        let header_len = version_header(version).len();
        if payload.starts_with(version_bytes(version))
            || (dev_type == DevType::Device22 && payload.len() & 0x0F != 0)
        {
            payload = payload.get(header_len..).unwrap_or_default().to_vec();
        }
        if version == ProtocolVersion::V33 {
            payload =
                aes_ecb_decrypt(key, &payload, false).map_err(|_| ProtoError::UnexpectedPayload)?;
        }
        if matches!(version, ProtocolVersion::V33 | ProtocolVersion::V34)
            && payload.windows(12).any(|w| w == b"data unvalid")
        {
            return Err(ProtoError::DataUnvalid);
        }
    } else if !payload.starts_with(b"{") {
        return Err(ProtoError::UnexpectedPayload);
    }

    let text = match String::from_utf8(payload) {
        Ok(text) => text,
        Err(e) => {
            let bytes = e.into_bytes();
            if bytes.first() == Some(&b'{') && bytes.last() == Some(&b'}') {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                return Err(ProtoError::BadJson("payload no UTF-8".into()));
            }
        }
    };
    let mut json: Value =
        serde_json::from_str(&text).map_err(|e| ProtoError::BadJson(e.to_string()))?;
    if let Value::Object(map) = &mut json
        && !map.contains_key("dps")
        && let Some(dps) = map.get("data").and_then(|d| d.get("dps")).cloned()
    {
        map.insert("dps".into(), dps);
    }
    Ok(Some(json))
}
