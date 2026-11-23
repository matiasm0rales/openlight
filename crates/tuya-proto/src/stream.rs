//! Decodificador de stream incremental, como `XenonDevice._receive`.
//!
//! Acepta bytes en pedazos de cualquier tamaño y varios frames juntos. Si el búfer no empieza
//! con un prefijo, descarta hasta el primer `0x000055AA` o `0x00006699`; si no hay ninguno, se
//! queda solo con los últimos 3 bytes (por si un prefijo quedó cortado).
//!
//! Diferencias con tinytuya, que lee de un socket bloqueante:
//! - No espera 28 bytes antes de buscar: le alcanza con el encabezado para saber el largo.
//! - Un encabezado con largo imposible (más que el tope) no corta el stream: se devuelve el
//!   error, se descarta el prefijo y se sigue buscando.

use crate::consts::{MAX_PAYLOAD_LENGTH, PREFIX_55AA_BIN, PREFIX_6699_BIN};
use crate::crypto::Key;
use crate::error::{ProtoError, Result};
use crate::frame::{
    DecodedFrame, Prefix, RetcodeMode, parse_header_with_limit, unpack_message_with_header,
};

/// Error de un frame del stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StreamError {
    /// El frame estaba completo pero no se pudo desarmar (largo imposible, etc.).
    #[error(transparent)]
    Proto(#[from] ProtoError),
    /// CRC32/HMAC inválido en un frame 55AA (clave equivocada o frame corrupto). Trae el frame
    /// desarmado por si se lo quiere usar igual, como hace tinytuya.
    #[error("CRC/HMAC inválido")]
    BadChecksum(Box<DecodedFrame>),
    /// Tag GCM inválido en un frame 6699: el payload es basura cifrada (tinytuya lo rechaza).
    #[error("tag GCM inválido")]
    AuthFailed(Box<DecodedFrame>),
}

/// Búfer de bytes recibidos que devuelve frames completos.
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    buf: Vec<u8>,
    max_payload: u32,
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn find(haystack: &[u8], needle: &[u8; 4]) -> Option<usize> {
    haystack.windows(4).position(|w| w == needle)
}

impl StreamDecoder {
    /// Decodificador vacío con el tope [`MAX_PAYLOAD_LENGTH`].
    pub fn new() -> Self {
        Self::with_max_payload(MAX_PAYLOAD_LENGTH)
    }

    /// Decodificador con otro tope de payload.
    pub fn with_max_payload(max_payload: u32) -> Self {
        Self {
            buf: Vec::new(),
            max_payload,
        }
    }

    /// Agrega bytes recibidos.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes que esperan ser procesados.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Vacía el búfer (por ejemplo al reconectar).
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Descarta basura hasta el primer prefijo. Devuelve el formato si hay uno al principio.
    fn resync(&mut self) -> Option<Prefix> {
        let a = find(&self.buf, &PREFIX_55AA_BIN);
        let b = find(&self.buf, &PREFIX_6699_BIN);
        let offset = match (a, b) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => {
                let keep = self.buf.len().min(3);
                self.buf.drain(..self.buf.len() - keep);
                return None;
            }
        };
        self.buf.drain(..offset);
        Some(if self.buf.starts_with(&PREFIX_6699_BIN) {
            Prefix::P6699
        } else {
            Prefix::P55aa
        })
    }

    /// Devuelve el próximo frame completo, o `None` si faltan bytes.
    ///
    /// `key` es la clave vigente (HMAC en 3.4, GCM en 3.5; `None` en 3.1/3.3) y `retcode`
    /// dice si los frames traen retcode ([`RetcodeMode::Present`] para lo que manda el foco,
    /// [`RetcodeMode::Absent`] para lo que manda la app). Se pasan en cada llamada porque la
    /// clave cambia a mitad del stream al negociar la sesión.
    ///
    /// Un frame con error se consume igual: la próxima llamada sigue con lo que viene detrás.
    pub fn next_frame(
        &mut self,
        key: Option<&Key>,
        retcode: RetcodeMode,
    ) -> Option<Result<DecodedFrame, StreamError>> {
        let prefix = self.resync()?;
        if self.buf.len() < prefix.header_len() {
            return None;
        }
        let header = match parse_header_with_limit(&self.buf, self.max_payload) {
            Ok(h) => h,
            Err(e) => {
                // Prefijo falso o largo imposible: saltear el prefijo y resincronizar.
                self.buf.drain(..1);
                return Some(Err(e.into()));
            }
        };
        if self.buf.len() < header.total_length {
            return None;
        }
        let frame: Vec<u8> = self.buf.drain(..header.total_length).collect();
        Some(
            match unpack_message_with_header(&frame, &header, key, retcode) {
                Ok(d) if d.crc_good => Ok(d),
                Ok(d) if d.prefix == Prefix::P6699 => Err(StreamError::AuthFailed(Box::new(d))),
                Ok(d) => Err(StreamError::BadChecksum(Box::new(d))),
                Err(e) => Err(e.into()),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Message, pack_message};

    const KEY: Key = *b"0123456789abcdef";

    fn frames() -> Vec<Vec<u8>> {
        let m = |seqno, prefix| Message {
            seqno,
            cmd: 8,
            retcode: Some(0),
            payload: format!("{{\"n\":{seqno}}}").into_bytes(),
            prefix,
            iv: Some([seqno as u8; 12]),
        };
        vec![
            pack_message(&m(1, Prefix::P55aa), Some(&KEY)).unwrap(),
            pack_message(&m(2, Prefix::P6699), Some(&KEY)).unwrap(),
            pack_message(&m(3, Prefix::P55aa), Some(&KEY)).unwrap(),
        ]
    }

    fn drain(dec: &mut StreamDecoder) -> Vec<u32> {
        let mut out = Vec::new();
        while let Some(r) = dec.next_frame(Some(&KEY), RetcodeMode::Present) {
            out.push(r.unwrap().seqno);
        }
        out
    }

    #[test]
    fn de_a_un_byte_y_con_basura_en_el_medio() {
        let mut all = vec![1, 2, 3, 0, 0];
        for f in frames() {
            all.extend_from_slice(&f);
            all.extend_from_slice(b"\x00\x00\x55basura");
        }
        let mut dec = StreamDecoder::new();
        let mut seen = Vec::new();
        for b in &all {
            dec.push(&[*b]);
            seen.extend(drain(&mut dec));
        }
        assert_eq!(seen, vec![1, 2, 3]);
        assert!(dec.buffered() <= 3);

        let mut dec = StreamDecoder::new();
        dec.push(&all);
        assert_eq!(drain(&mut dec), vec![1, 2, 3]);
    }

    #[test]
    fn clave_equivocada_y_largo_imposible() {
        let otra = *b"fedcba9876543210";
        let mut dec = StreamDecoder::new();
        for f in frames() {
            dec.push(&f);
        }
        assert!(matches!(
            dec.next_frame(Some(&otra), RetcodeMode::Present),
            Some(Err(StreamError::BadChecksum(_)))
        ));
        assert!(matches!(
            dec.next_frame(Some(&otra), RetcodeMode::Present),
            Some(Err(StreamError::AuthFailed(_)))
        ));
        assert_eq!(
            dec.next_frame(Some(&KEY), RetcodeMode::Present)
                .unwrap()
                .unwrap()
                .seqno,
            3
        );

        let mut f = frames().remove(0);
        f[12..16].copy_from_slice(&5000u32.to_be_bytes());
        dec.push(&f);
        dec.push(&frames()[2]);
        assert!(matches!(
            dec.next_frame(Some(&KEY), RetcodeMode::Present),
            Some(Err(StreamError::Proto(ProtoError::PayloadTooLarge(5000))))
        ));
        assert_eq!(drain(&mut dec), vec![3]);
    }
}
