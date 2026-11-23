//! Estado de la conexión del lado de la app, sin IO: versión, claves, `seqno` y negociación.

use openlight_types::ProtocolVersion;
use serde_json::Value;

use crate::codec::{decode_payload, encode_message};
use crate::crypto::Key;
use crate::error::{ProtoError, Result};
use crate::frame::DecodedFrame;
use crate::negotiation::{derive_session_key, step1, step3};
use crate::payload::{DevType, MessagePayload, PayloadRequest, generate_payload};

/// Una conexión con un foco, como el estado que guarda `XenonDevice` (sin el socket).
///
/// El `seqno` empieza en 1 y sube con cada [`ClientSession::encode`].
#[derive(Debug, Clone)]
pub struct ClientSession {
    version: ProtocolVersion,
    real_key: Key,
    key: Key,
    seqno: u32,
    dev_type: DevType,
    local_nonce: Option<[u8; 16]>,
    remote_nonce: Option<[u8; 16]>,
}

impl ClientSession {
    /// Sesión nueva con la local_key.
    pub fn new(version: ProtocolVersion, local_key: Key) -> Self {
        Self {
            version,
            real_key: local_key,
            key: local_key,
            seqno: 1,
            dev_type: DevType::Default,
            local_nonce: None,
            remote_nonce: None,
        }
    }

    /// Versión del protocolo.
    pub fn version(&self) -> ProtocolVersion {
        self.version
    }

    /// Próximo `seqno`.
    pub fn seqno(&self) -> u32 {
        self.seqno
    }

    /// Clave vigente: la de sesión si ya se negoció, si no la local_key.
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// Clave para [`crate::StreamDecoder::next_frame`]: `None` en 3.1/3.3 (CRC32).
    pub fn frame_key(&self) -> Option<Key> {
        self.version.uses_session_key().then_some(self.key)
    }

    /// Tipo de dispositivo.
    pub fn dev_type(&self) -> DevType {
        self.dev_type
    }

    /// Cambia el tipo de dispositivo (tras un [`ProtoError::DataUnvalid`], pasar a device22).
    pub fn set_dev_type(&mut self, dev_type: DevType) {
        self.dev_type = dev_type;
    }

    /// Si la versión necesita negociar y todavía no se hizo.
    pub fn needs_negotiation(&self) -> bool {
        self.version.uses_session_key() && self.remote_nonce.is_none()
    }

    /// Arma el payload de un pedido con la versión y el tipo de esta sesión.
    pub fn payload(&self, req: &PayloadRequest<'_>) -> MessagePayload {
        let req = PayloadRequest {
            version: self.version,
            dev_type: self.dev_type,
            ..req.clone()
        };
        generate_payload(&req)
    }

    /// Paso 1: vuelve a la local_key y devuelve `SESS_KEY_NEG_START` con `local_nonce`
    /// (que tiene que ser aleatorio en producción).
    pub fn start_negotiation(&mut self, local_nonce: [u8; 16]) -> MessagePayload {
        self.key = self.real_key;
        self.local_nonce = Some(local_nonce);
        self.remote_nonce = None;
        step1(&local_nonce)
    }

    /// Paso 3: valida la respuesta del foco y devuelve el
    /// `SESS_KEY_NEG_FINISH` a mandar. Como en tinytuya, el FINISH todavía va con la
    /// local_key: codificarlo con [`ClientSession::encode`] y recién después llamar a
    /// [`ClientSession::finish_negotiation`].
    ///
    /// # Errors
    /// [`ProtoError::Negotiation`] si no se llamó a [`ClientSession::start_negotiation`] o la
    /// respuesta es inválida.
    pub fn handle_negotiation_response(
        &mut self,
        response: &DecodedFrame,
    ) -> Result<MessagePayload> {
        let local_nonce = self
            .local_nonce
            .ok_or(ProtoError::Negotiation("no se empezó la negociación"))?;
        let (remote_nonce, finish) = step3(self.version, &self.real_key, &local_nonce, response)?;
        self.remote_nonce = Some(remote_nonce);
        Ok(finish)
    }

    /// Paso 4: pasa a la clave de sesión. Llamarlo después de codificar el FINISH.
    ///
    /// # Errors
    /// [`ProtoError::Negotiation`] si falta algún nonce.
    pub fn finish_negotiation(&mut self) -> Result<Key> {
        let (Some(local), Some(remote)) = (self.local_nonce, self.remote_nonce) else {
            return Err(ProtoError::Negotiation("faltan los nonces"));
        };
        self.key = derive_session_key(self.version, &self.real_key, &local, &remote);
        Ok(self.key)
    }

    /// Cifra y empaqueta con la clave vigente y el próximo `seqno` (`_encode_message`).
    /// `iv` es obligatorio en 3.5.
    ///
    /// # Errors
    /// [`ProtoError::MissingIv`] en 3.5 sin IV.
    pub fn encode(&mut self, msg: &MessagePayload, iv: Option<[u8; 12]>) -> Result<Vec<u8>> {
        let key = self.key;
        self.encode_with_key(&key, msg, iv)
    }

    /// Como [`ClientSession::encode`] pero con otra clave.
    ///
    /// # Errors
    /// [`ProtoError::MissingIv`] en 3.5 sin IV.
    pub fn encode_with_key(
        &mut self,
        key: &Key,
        msg: &MessagePayload,
        iv: Option<[u8; 12]>,
    ) -> Result<Vec<u8>> {
        let frame = encode_message(self.version, key, self.seqno, msg, iv)?;
        self.seqno = self.seqno.wrapping_add(1);
        Ok(frame)
    }

    /// Descifra y parsea el payload de un frame del foco (`_decode_payload`).
    ///
    /// # Errors
    /// Ver [`decode_payload`].
    pub fn decode(&self, frame: &DecodedFrame) -> Result<Option<Value>> {
        decode_payload(self.version, &self.key, self.dev_type, &frame.payload)
    }
}
