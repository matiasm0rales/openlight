//! Frames 55AA (3.1–3.4) y 6699 (3.5): `pack_message`, `parse_header` y `unpack_message` de
//! `tinytuya/core/message_helper.py`.
//!
//! ```text
//! 55AA: prefijo u32 | seq u32 | cmd u32 | len u32 | [retcode u32] | payload | CRC32 u32 o HMAC 32 | sufijo
//!       len = [retcode] + payload + CRC/HMAC + sufijo
//! 6699: prefijo u32 | 0 u16 | seq u32 | cmd u32 | len u32 | IV 12 | GCM([retcode] + payload) | tag 16 | sufijo
//!       len = IV + [retcode] + payload + tag   (AAD = encabezado sin el prefijo, 14 bytes)
//! ```

use crate::consts::{
    GCM_IV_LEN, GCM_TAG_LEN, HEADER_LEN_55AA, HEADER_LEN_6699, MAX_PAYLOAD_LENGTH, PREFIX_55AA,
    PREFIX_6699, PREFIX_6699_BIN, RETCODE_LEN, SUFFIX_55AA, SUFFIX_6699_BIN,
};
use crate::crypto::{Key, aes_gcm_decrypt, aes_gcm_encrypt, crc32, hmac_sha256};
use crate::error::{ProtoError, Result};

/// Formato del frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prefix {
    /// `0x000055AA` (3.1–3.4).
    P55aa,
    /// `0x00006699` (3.5).
    P6699,
}

impl Prefix {
    /// Valor del prefijo.
    pub const fn value(self) -> u32 {
        match self {
            Self::P55aa => PREFIX_55AA,
            Self::P6699 => PREFIX_6699,
        }
    }

    /// Largo del encabezado.
    pub const fn header_len(self) -> usize {
        match self {
            Self::P55aa => HEADER_LEN_55AA,
            Self::P6699 => HEADER_LEN_6699,
        }
    }
}

/// Un mensaje antes de empaquetar (el `TuyaMessage` de tinytuya, sin `crc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Número de secuencia.
    pub seqno: u32,
    /// Comando ([`crate::cmd`]).
    pub cmd: u32,
    /// Retcode: `Some` en lo que manda el foco, `None` en lo que manda la app.
    ///
    /// En 55AA tinytuya espera que el retcode venga dentro del payload; acá se agrega
    /// adelante del payload, que produce los mismos bytes.
    pub retcode: Option<u32>,
    /// Payload (ya cifrado en 3.1–3.4; en claro en 3.5, que se cifra al empaquetar).
    pub payload: Vec<u8>,
    /// Formato.
    pub prefix: Prefix,
    /// IV de GCM (solo 6699; obligatorio ahí).
    pub iv: Option<[u8; 12]>,
}

/// Encabezado leído de un frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Formato.
    pub prefix: Prefix,
    /// Número de secuencia.
    pub seqno: u32,
    /// Comando.
    pub cmd: u32,
    /// Campo `len` del encabezado.
    pub length: u32,
    /// Largo total del frame, con encabezado y sufijo.
    pub total_length: usize,
}

/// Cómo leer el retcode (el parámetro `no_retcode` de `unpack_message`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetcodeMode {
    /// Lo que manda el foco: siempre trae retcode (`no_retcode=False`).
    Present,
    /// Lo que manda la app: no trae retcode (`no_retcode=True`).
    Absent,
    /// Adivinar (`no_retcode=None`): en 6699 hay retcode si el payload no empieza con `{` y el
    /// byte 4 sí; en 55AA equivale a [`RetcodeMode::Present`]. Lo usan los broadcasts UDP.
    Detect,
}

/// Un frame desarmado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    /// Número de secuencia.
    pub seqno: u32,
    /// Comando.
    pub cmd: u32,
    /// Retcode (0 si el frame no lo trae).
    pub retcode: u32,
    /// Payload sin retcode ni CRC/HMAC/tag. En 6699 ya viene descifrado si `crc_good`.
    pub payload: Vec<u8>,
    /// CRC/HMAC correcto, o tag GCM válido.
    ///
    /// Como en tinytuya, un CRC/HMAC malo en 55AA no es un error: queda a criterio de quien
    /// llama. En 6699 con tag inválido el payload queda cifrado.
    pub crc_good: bool,
    /// Formato.
    pub prefix: Prefix,
    /// IV de GCM (solo 6699).
    pub iv: Option<[u8; 12]>,
    /// Largo total del frame.
    pub total_length: usize,
}

fn be_u32(data: &[u8], at: usize) -> Result<u32> {
    let bytes = data.get(at..at + 4).ok_or(ProtoError::NotEnoughData)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Empaqueta un mensaje (`pack_message`).
///
/// En 55AA, `key` elige HMAC-SHA256 (3.4) en vez de CRC32. En 6699 la clave es obligatoria y
/// cifra con GCM usando `msg.iv`.
///
/// # Errors
/// [`ProtoError::MissingKey`] o [`ProtoError::MissingIv`] en 6699 sin clave o sin IV.
pub fn pack_message(msg: &Message, key: Option<&Key>) -> Result<Vec<u8>> {
    let mut raw = Vec::with_capacity(RETCODE_LEN + msg.payload.len());
    if let Some(retcode) = msg.retcode {
        raw.extend_from_slice(&retcode.to_be_bytes());
    }
    raw.extend_from_slice(&msg.payload);

    match msg.prefix {
        Prefix::P55aa => {
            let end_len = if key.is_some() { 32 + 4 } else { 4 + 4 };
            let len =
                u32::try_from(raw.len() + end_len).map_err(|_| ProtoError::BadLength(u32::MAX))?;
            let mut data = Vec::with_capacity(HEADER_LEN_55AA + raw.len() + end_len);
            data.extend_from_slice(&PREFIX_55AA.to_be_bytes());
            data.extend_from_slice(&msg.seqno.to_be_bytes());
            data.extend_from_slice(&msg.cmd.to_be_bytes());
            data.extend_from_slice(&len.to_be_bytes());
            data.extend_from_slice(&raw);
            match key {
                Some(k) => {
                    let mac = hmac_sha256(k, &data);
                    data.extend_from_slice(&mac);
                }
                None => {
                    let crc = crc32(&data);
                    data.extend_from_slice(&crc.to_be_bytes());
                }
            }
            data.extend_from_slice(&SUFFIX_55AA.to_be_bytes());
            Ok(data)
        }
        Prefix::P6699 => {
            let key = key.ok_or(ProtoError::MissingKey)?;
            let iv = msg.iv.ok_or(ProtoError::MissingIv)?;
            let len = u32::try_from(raw.len() + GCM_TAG_LEN + GCM_IV_LEN)
                .map_err(|_| ProtoError::BadLength(u32::MAX))?;
            let mut data =
                Vec::with_capacity(HEADER_LEN_6699 + GCM_IV_LEN + raw.len() + GCM_TAG_LEN + 4);
            data.extend_from_slice(&PREFIX_6699.to_be_bytes());
            data.extend_from_slice(&0u16.to_be_bytes());
            data.extend_from_slice(&msg.seqno.to_be_bytes());
            data.extend_from_slice(&msg.cmd.to_be_bytes());
            data.extend_from_slice(&len.to_be_bytes());
            let (ct, tag) = aes_gcm_encrypt(key, &iv, &data[4..], &raw);
            data.extend_from_slice(&iv);
            data.extend_from_slice(&ct);
            data.extend_from_slice(&tag);
            data.extend_from_slice(&SUFFIX_6699_BIN);
            Ok(data)
        }
    }
}

/// Lee el encabezado (`parse_header`) con el tope [`MAX_PAYLOAD_LENGTH`].
///
/// # Errors
/// Ver [`parse_header_with_limit`].
pub fn parse_header(data: &[u8]) -> Result<Header> {
    parse_header_with_limit(data, MAX_PAYLOAD_LENGTH)
}

/// Lee el encabezado con otro tope de payload.
///
/// # Errors
/// [`ProtoError::NotEnoughData`] si no alcanza para el encabezado, [`ProtoError::BadPrefix`]
/// si el prefijo es otro y [`ProtoError::PayloadTooLarge`] si `len` supera `max_payload`.
pub fn parse_header_with_limit(data: &[u8], max_payload: u32) -> Result<Header> {
    let prefix = if data.get(..4) == Some(&PREFIX_6699_BIN[..]) {
        Prefix::P6699
    } else {
        Prefix::P55aa
    };
    let header_len = prefix.header_len();
    if data.len() < header_len {
        return Err(ProtoError::NotEnoughData);
    }
    let value = be_u32(data, 0)?;
    let (seqno, cmd, length, total_length) = match prefix {
        Prefix::P55aa => {
            if value != PREFIX_55AA {
                return Err(ProtoError::BadPrefix(value));
            }
            let length = be_u32(data, 12)?;
            (
                be_u32(data, 4)?,
                be_u32(data, 8)?,
                length,
                length as usize + header_len,
            )
        }
        Prefix::P6699 => {
            let length = be_u32(data, 14)?;
            (
                be_u32(data, 6)?,
                be_u32(data, 10)?,
                length,
                length as usize + header_len + 4,
            )
        }
    };
    if length > max_payload {
        return Err(ProtoError::PayloadTooLarge(length));
    }
    Ok(Header {
        prefix,
        seqno,
        cmd,
        length,
        total_length,
    })
}

/// Desarma un frame (`unpack_message`). Lee el encabezado con [`parse_header`].
///
/// # Errors
/// Ver [`unpack_message_with_header`].
pub fn unpack_message(
    data: &[u8],
    key: Option<&Key>,
    retcode: RetcodeMode,
) -> Result<DecodedFrame> {
    let header = parse_header(data)?;
    unpack_message_with_header(data, &header, key, retcode)
}

/// Desarma un frame con un encabezado ya leído.
///
/// En 55AA, `key` elige verificar HMAC-SHA256 en vez de CRC32; en 6699 es obligatoria.
///
/// # Errors
/// [`ProtoError::NotEnoughData`] si faltan bytes, [`ProtoError::BadLength`] si el largo
/// declarado no alcanza para lo fijo del frame y [`ProtoError::MissingKey`] en 6699 sin clave.
pub fn unpack_message_with_header(
    data: &[u8],
    header: &Header,
    key: Option<&Key>,
    retcode: RetcodeMode,
) -> Result<DecodedFrame> {
    let length = header.length as usize;
    match header.prefix {
        Prefix::P55aa => {
            let header_len = HEADER_LEN_55AA;
            let end_len = if key.is_some() { 32 + 4 } else { 4 + 4 };
            let retcode_len = if retcode == RetcodeMode::Absent {
                0
            } else {
                RETCODE_LEN
            };
            let msg_len = header_len + length;
            if data.len() < msg_len {
                return Err(ProtoError::NotEnoughData);
            }
            if length < retcode_len + end_len {
                return Err(ProtoError::BadLength(header.length));
            }
            let code = if retcode_len == 0 {
                0
            } else {
                be_u32(data, header_len)?
            };
            let body_end = msg_len - end_len;
            let payload = data[header_len + retcode_len..body_end].to_vec();
            let signed = &data[..body_end];
            let crc_good = match key {
                Some(k) => hmac_sha256(k, signed)[..] == data[body_end..body_end + 32],
                None => crc32(signed) == be_u32(data, body_end)?,
            };
            // Un sufijo distinto solo se registra en tinytuya: no invalida el frame.
            Ok(DecodedFrame {
                seqno: header.seqno,
                cmd: header.cmd,
                retcode: code,
                payload,
                crc_good,
                prefix: Prefix::P55aa,
                iv: None,
                total_length: msg_len,
            })
        }
        Prefix::P6699 => {
            let key = key.ok_or(ProtoError::MissingKey)?;
            let header_len = HEADER_LEN_6699;
            let end_len = GCM_TAG_LEN + 4;
            let msg_len = header_len + length + 4;
            if data.len() < msg_len {
                return Err(ProtoError::NotEnoughData);
            }
            if length < end_len {
                return Err(ProtoError::BadLength(header.length));
            }
            let body = &data[header_len..msg_len - end_len];
            let mut tag = [0u8; 16];
            tag.copy_from_slice(&data[msg_len - end_len..msg_len - 4]);
            let (iv, mut payload, crc_good) = match body.split_first_chunk::<GCM_IV_LEN>() {
                Some((iv, ct)) => match aes_gcm_decrypt(key, iv, &data[4..header_len], ct, &tag) {
                    Ok(plain) => (Some(*iv), plain, true),
                    Err(_) => (Some(*iv), ct.to_vec(), false),
                },
                // IV truncado: tinytuya no logra descifrar y marca el frame como malo.
                None => (None, Vec::new(), false),
            };
            let has_retcode = match retcode {
                RetcodeMode::Present => true,
                RetcodeMode::Absent => false,
                RetcodeMode::Detect => {
                    payload.first() != Some(&b'{') && payload.get(4) == Some(&b'{')
                }
            };
            let code = if has_retcode {
                let code = be_u32(&payload, 0)?;
                payload.drain(..RETCODE_LEN);
                code
            } else {
                0
            };
            Ok(DecodedFrame {
                seqno: header.seqno,
                cmd: header.cmd,
                retcode: code,
                payload,
                crc_good,
                prefix: Prefix::P6699,
                iv,
                total_length: msg_len,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: Key = *b"0123456789abcdef";

    fn msg(prefix: Prefix, retcode: Option<u32>) -> Message {
        Message {
            seqno: 7,
            cmd: 9,
            retcode,
            payload: b"{\"a\":1}".to_vec(),
            prefix,
            iv: Some(*b"0123456789ab"),
        }
    }

    #[test]
    fn ida_y_vuelta_en_los_dos_formatos() {
        for (prefix, key) in [
            (Prefix::P55aa, None),
            (Prefix::P55aa, Some(&KEY)),
            (Prefix::P6699, Some(&KEY)),
        ] {
            for (retcode, mode) in [(Some(3), RetcodeMode::Present), (None, RetcodeMode::Absent)] {
                let m = msg(prefix, retcode);
                let frame = pack_message(&m, key).unwrap();
                let header = parse_header(&frame).unwrap();
                assert_eq!(header.total_length, frame.len());
                let d = unpack_message(&frame, key, mode).unwrap();
                assert!(d.crc_good);
                assert_eq!((d.seqno, d.cmd, d.retcode), (7, 9, retcode.unwrap_or(0)));
                assert_eq!(d.payload, m.payload);
                assert_eq!(d.total_length, frame.len());
            }
        }
    }

    #[test]
    fn detecta_el_retcode_en_6699() {
        for retcode in [Some(0), None] {
            let frame = pack_message(&msg(Prefix::P6699, retcode), Some(&KEY)).unwrap();
            let d = unpack_message(&frame, Some(&KEY), RetcodeMode::Detect).unwrap();
            assert_eq!(d.payload, b"{\"a\":1}");
        }
    }

    #[test]
    fn un_crc_malo_no_es_error_pero_queda_marcado() {
        let mut frame = pack_message(&msg(Prefix::P55aa, Some(0)), None).unwrap();
        let n = frame.len();
        frame[n - 5] ^= 1;
        assert!(
            !unpack_message(&frame, None, RetcodeMode::Present)
                .unwrap()
                .crc_good
        );
        let mut frame = pack_message(&msg(Prefix::P6699, Some(0)), Some(&KEY)).unwrap();
        frame[20] ^= 1;
        assert!(
            !unpack_message(&frame, Some(&KEY), RetcodeMode::Absent)
                .unwrap()
                .crc_good
        );
    }

    #[test]
    fn errores_de_encabezado() {
        assert_eq!(parse_header(&[0, 0, 0x55]), Err(ProtoError::NotEnoughData));
        assert_eq!(
            parse_header(&[1; 16]),
            Err(ProtoError::BadPrefix(0x0101_0101))
        );
        let mut frame = pack_message(&msg(Prefix::P55aa, None), None).unwrap();
        frame[12..16].copy_from_slice(&2000u32.to_be_bytes());
        assert_eq!(parse_header(&frame), Err(ProtoError::PayloadTooLarge(2000)));
        frame[12..16].copy_from_slice(&4u32.to_be_bytes());
        assert_eq!(
            unpack_message(&frame, None, RetcodeMode::Present),
            Err(ProtoError::BadLength(4))
        );
        assert_eq!(
            pack_message(&msg(Prefix::P6699, None), None),
            Err(ProtoError::MissingKey)
        );
        let sin_iv = Message {
            iv: None,
            ..msg(Prefix::P6699, None)
        };
        assert_eq!(
            pack_message(&sin_iv, Some(&KEY)),
            Err(ProtoError::MissingIv)
        );
    }
}
