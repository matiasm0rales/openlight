//! [`Suite`] real, sobre `tuya-proto` (tarea A). Acá no hay framing ni cripto propios: todo
//! delega en `tuya-proto`.

use openlight_types::ProtocolVersion;
use tuya_proto::crypto::{aes_ecb_decrypt, hmac_sha256};
use tuya_proto::udp::{BroadcastKind, decrypt_udp, encode_broadcast};
use tuya_proto::{
    DeviceMessage, Key, MessagePayload, RetcodeMode, StreamDecoder, StreamError,
    decode_client_payload, decode_payload, derive_session_key, encode_device_message,
    encode_message, parse_header,
};

use crate::wire::{Msg, Role, Suite, Wire, WireError, cmd};

/// La [`Suite`] de verdad: frames y cripto de tinytuya, implementados por `tuya-proto`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProtoSuite;

fn random_iv() -> [u8; 12] {
    let mut iv = [0u8; 12];
    getrandom::fill(&mut iv).expect("getrandom");
    iv
}

struct ProtoWire {
    version: ProtocolVersion,
    key: Key,
    role: Role,
    stream: StreamDecoder,
}

impl ProtoWire {
    /// Clave con la que se autentica el frame: HMAC en 3.4, GCM en 3.5, CRC en el resto.
    fn frame_key(&self) -> Option<&Key> {
        self.version.uses_session_key().then_some(&self.key)
    }

    fn plaintext(&self, cmd_id: u32, payload: &[u8]) -> Result<Vec<u8>, WireError> {
        if payload.is_empty() {
            return Ok(Vec::new());
        }
        match self.role {
            // Lo que manda la app: cualquier falla de descifrado es "otra clave".
            Role::Device => decode_client_payload(self.version, &self.key, payload)
                .map_err(|_| WireError::BadAuth),
            Role::Client => {
                if cmd_id == cmd::SESS_KEY_NEG_RESP {
                    // Binario (nonce + HMAC): en 3.4 va cifrado con ECB, en 3.5 ya viene en claro.
                    return if self.version == ProtocolVersion::V34 {
                        aes_ecb_decrypt(&self.key, payload, false).map_err(|_| WireError::BadAuth)
                    } else {
                        Ok(payload.to_vec())
                    };
                }
                let value = decode_payload(self.version, &self.key, Default::default(), payload)
                    .map_err(|_| WireError::BadAuth)?;
                Ok(value
                    .map(|v| v.to_string().into_bytes())
                    .unwrap_or_default())
            }
        }
    }
}

impl Wire for ProtoWire {
    fn decode(&mut self, buf: &mut Vec<u8>) -> Result<Option<Msg>, WireError> {
        self.stream.push(buf);
        buf.clear();
        let mode = match self.role {
            Role::Device => RetcodeMode::Absent,
            Role::Client => RetcodeMode::Present,
        };
        let key = self.frame_key().copied();
        match self.stream.next_frame(key.as_ref(), mode) {
            None => Ok(None),
            Some(Err(StreamError::BadChecksum(_) | StreamError::AuthFailed(_))) => {
                Err(WireError::BadAuth)
            }
            Some(Err(StreamError::Proto(e))) => Err(WireError::Malformed(e.to_string())),
            Some(Ok(frame)) => {
                let payload = self.plaintext(frame.cmd, &frame.payload)?;
                Ok(Some(Msg {
                    seqno: frame.seqno,
                    cmd: frame.cmd,
                    retcode: (self.role == Role::Client).then_some(frame.retcode),
                    payload,
                }))
            }
        }
    }

    fn encode(&mut self, msg: &Msg, with_header: bool) -> Vec<u8> {
        let iv = Some(random_iv());
        let out = match self.role {
            Role::Device => encode_device_message(
                self.version,
                &self.key,
                &DeviceMessage {
                    seqno: msg.seqno,
                    cmd: msg.cmd,
                    retcode: msg.retcode.unwrap_or(0),
                    body: &msg.payload,
                    with_header,
                },
                iv,
            ),
            // `encode_message` decide solo qué comandos llevan el encabezado de versión.
            Role::Client => encode_message(
                self.version,
                &self.key,
                msg.seqno,
                &MessagePayload::new(msg.cmd, msg.payload.clone()),
                iv,
            ),
        };
        // Con clave de 16 bytes e IV, armar un frame no falla.
        out.expect("tuya-proto armó el frame")
    }

    fn set_key(&mut self, key: [u8; 16]) {
        self.key = key;
    }
}

impl Suite for ProtoSuite {
    fn wire(&self, version: ProtocolVersion, key: [u8; 16], role: Role) -> Box<dyn Wire> {
        Box::new(ProtoWire {
            version,
            key,
            role,
            stream: StreamDecoder::new(),
        })
    }

    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> [u8; 32] {
        hmac_sha256(key, data)
    }

    fn session_key(
        &self,
        version: ProtocolVersion,
        local_key: &[u8; 16],
        client_nonce: &[u8; 16],
        device_nonce: &[u8; 16],
    ) -> [u8; 16] {
        derive_session_key(version, local_key, client_nonce, device_nonce)
    }

    fn udp_encode(&self, version: ProtocolVersion, payload: &[u8]) -> Vec<u8> {
        let kind = match version {
            ProtocolVersion::V31 => BroadcastKind::Plain6666,
            ProtocolVersion::V33 | ProtocolVersion::V34 => BroadcastKind::Encrypted6667,
            ProtocolVersion::V35 => BroadcastKind::Gcm7000,
        };
        encode_broadcast(kind, payload, Some(random_iv()))
    }

    fn udp_decode(&self, frame: &[u8]) -> Option<(u32, Vec<u8>)> {
        let header = parse_header(frame).ok()?;
        let text = decrypt_udp(frame).ok()?;
        Some((header.cmd, text.into_bytes()))
    }
}
