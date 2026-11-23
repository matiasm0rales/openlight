//! Lado del foco: armar lo que manda el foco y descifrar lo que manda la app. Lo usa `tuya-sim`.
//!
//! [`encode_device_message`] reproduce `frame_del_foco` de `tools/golden/gen_proto.py`, que arma
//! los frames con `pack_message` de tinytuya; los `device_frames` de las fixtures lo verifican
//! byte a byte.

use openlight_types::ProtocolVersion;

use crate::consts::{version_bytes, version_header};
use crate::crypto::{Key, aes_ecb_decrypt, aes_ecb_encrypt, base64_decode};
use crate::error::Result;
use crate::frame::{Message, Prefix, pack_message};

/// Frame que manda el foco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceMessage<'a> {
    /// Número de secuencia.
    pub seqno: u32,
    /// Comando.
    pub cmd: u32,
    /// Retcode (0 = ok).
    pub retcode: u32,
    /// Cuerpo en claro (JSON, o vacío para el latido).
    pub body: &'a [u8],
    /// Si lleva el encabezado de versión `"3.x"` + 12×0 (las notificaciones STATUS de 3.3+).
    pub with_header: bool,
}

/// Arma un frame del foco.
///
/// | versión | frame |
/// |---|---|
/// | 3.1 | 55AA, retcode + cuerpo en claro, CRC32 (el encabezado de versión no se usa) |
/// | 3.3 | 55AA, retcode + [encabezado] + ECB(cuerpo), CRC32 |
/// | 3.4 | 55AA, retcode + ECB([encabezado] + cuerpo), HMAC-SHA256 |
/// | 3.5 | 6699, GCM(retcode + [encabezado] + cuerpo) con `iv` |
///
/// En 3.1–3.4 un cuerpo vacío va solo con el retcode, sin cifrar.
///
/// # Errors
/// [`crate::ProtoError::MissingIv`] en 3.5 sin IV.
pub fn encode_device_message(
    version: ProtocolVersion,
    key: &Key,
    msg: &DeviceMessage<'_>,
    iv: Option<[u8; 12]>,
) -> Result<Vec<u8>> {
    let header = version_header(version);
    let mut plain = Vec::with_capacity(header.len() + msg.body.len());
    if msg.with_header {
        plain.extend_from_slice(&header);
    }
    plain.extend_from_slice(msg.body);

    let frame = |payload: Vec<u8>, prefix, iv| Message {
        seqno: msg.seqno,
        cmd: msg.cmd,
        retcode: Some(msg.retcode),
        payload,
        prefix,
        iv,
    };
    let hmac_key = matches!(version, ProtocolVersion::V34 | ProtocolVersion::V35).then_some(key);

    if msg.body.is_empty() && version != ProtocolVersion::V35 {
        return pack_message(&frame(Vec::new(), Prefix::P55aa, None), hmac_key);
    }
    match version {
        ProtocolVersion::V35 => pack_message(&frame(plain, Prefix::P6699, iv), Some(key)),
        ProtocolVersion::V34 => {
            let ct = aes_ecb_encrypt(key, &plain, true)?;
            pack_message(&frame(ct, Prefix::P55aa, None), Some(key))
        }
        ProtocolVersion::V33 => {
            let mut payload = Vec::new();
            if msg.with_header {
                payload.extend_from_slice(&header);
            }
            payload.extend_from_slice(&aes_ecb_encrypt(key, msg.body, true)?);
            pack_message(&frame(payload, Prefix::P55aa, None), None)
        }
        ProtocolVersion::V31 => pack_message(&frame(msg.body.to_vec(), Prefix::P55aa, None), None),
    }
}

/// Descifra el payload de un frame de la app (lo inverso de [`crate::encode_message`]).
///
/// Recibe el payload del frame ya desarmado sin retcode ([`crate::RetcodeMode::Absent`]); en
/// 3.5 ya viene descifrado por GCM. Devuelve el texto en claro, sin encabezado de versión.
///
/// # Errors
/// Errores de base64, largo o relleno si el payload no se puede descifrar.
pub fn decode_client_payload(
    version: ProtocolVersion,
    key: &Key,
    payload: &[u8],
) -> Result<Vec<u8>> {
    let header_len = version_header(version).len();
    let strip = |p: Vec<u8>| -> Vec<u8> {
        if p.starts_with(version_bytes(version)) {
            p.get(header_len..).unwrap_or_default().to_vec()
        } else {
            p
        }
    };
    match version {
        ProtocolVersion::V31 => {
            if payload.starts_with(version_bytes(version)) {
                let b64 = payload.get(3 + 16..).unwrap_or_default();
                aes_ecb_decrypt(key, &base64_decode(b64)?, false)
            } else {
                Ok(payload.to_vec())
            }
        }
        ProtocolVersion::V33 => aes_ecb_decrypt(key, &strip(payload.to_vec()), false),
        ProtocolVersion::V34 => Ok(strip(aes_ecb_decrypt(key, payload, false)?)),
        ProtocolVersion::V35 => Ok(strip(payload.to_vec())),
    }
}
