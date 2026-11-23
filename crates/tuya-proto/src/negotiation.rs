//! Negociación de la clave de sesión de 3.4/3.5 (cmd 3 → 4 → 5), como funciones puras.
//!
//! Sigue `_negotiate_session_key_generate_step_1/3/finalize` de `XenonDevice.py`:
//!
//! 1. La app manda `SESS_KEY_NEG_START` con su nonce de 16 bytes.
//! 2. El foco responde `SESS_KEY_NEG_RESP` con `nonce_remoto (16) + HMAC(local_key, nonce_local) (32)`
//!    (en 3.4 cifrado con ECB, en 3.5 dentro del GCM del frame).
//! 3. La app verifica el HMAC y manda `SESS_KEY_NEG_FINISH` con `HMAC(local_key, nonce_remoto)`.
//! 4. Clave de sesión = `nonce_local XOR nonce_remoto` cifrado con la local_key: ECB en 3.4; en
//!    3.5 GCM con `iv = nonce_local[..12]`, quedándose con los 16 bytes cifrados (sin el tag).

use openlight_types::ProtocolVersion;

use crate::consts::cmd;
use crate::crypto::{Key, aes_ecb_decrypt, aes_ecb_encrypt, aes_gcm_encrypt, hmac_sha256};
use crate::error::{ProtoError, Result};
use crate::frame::DecodedFrame;
use crate::payload::MessagePayload;

/// Paso 1: el mensaje `SESS_KEY_NEG_START` con el nonce local.
pub fn step1(local_nonce: &[u8; 16]) -> MessagePayload {
    MessagePayload::new(cmd::SESS_KEY_NEG_START, local_nonce.to_vec())
}

/// Paso 3: valida la respuesta del foco (ya desarmada con la local_key) y devuelve el nonce
/// remoto y el mensaje `SESS_KEY_NEG_FINISH`.
///
/// # Errors
/// [`ProtoError::Negotiation`] si la respuesta es corta, trae otro comando, no se puede
/// descifrar o el HMAC no coincide.
pub fn step3(
    version: ProtocolVersion,
    local_key: &Key,
    local_nonce: &[u8; 16],
    response: &DecodedFrame,
) -> Result<([u8; 16], MessagePayload)> {
    if response.payload.len() < 48 {
        return Err(ProtoError::Negotiation(
            "respuesta del paso 2 demasiado corta",
        ));
    }
    if response.cmd != cmd::SESS_KEY_NEG_RESP {
        return Err(ProtoError::Negotiation("el paso 2 respondió otro comando"));
    }
    let payload = if version == ProtocolVersion::V34 {
        aes_ecb_decrypt(local_key, &response.payload, false)
            .map_err(|_| ProtoError::Negotiation("no se pudo descifrar el paso 2"))?
    } else {
        response.payload.clone()
    };
    if payload.len() < 48 {
        return Err(ProtoError::Negotiation(
            "respuesta del paso 2 demasiado corta",
        ));
    }
    let mut remote_nonce = [0u8; 16];
    remote_nonce.copy_from_slice(&payload[..16]);
    if hmac_sha256(local_key, local_nonce)[..] != payload[16..48] {
        return Err(ProtoError::Negotiation("HMAC del paso 2 inválido"));
    }
    let finish = MessagePayload::new(
        cmd::SESS_KEY_NEG_FINISH,
        hmac_sha256(local_key, &remote_nonce).to_vec(),
    );
    Ok((remote_nonce, finish))
}

/// Paso 4: deriva la clave de sesión.
pub fn derive_session_key(
    version: ProtocolVersion,
    local_key: &Key,
    local_nonce: &[u8; 16],
    remote_nonce: &[u8; 16],
) -> Key {
    let mut xored = [0u8; 16];
    for (out, (a, b)) in xored.iter_mut().zip(local_nonce.iter().zip(remote_nonce)) {
        *out = a ^ b;
    }
    let mut key = [0u8; 16];
    if version == ProtocolVersion::V35 {
        let mut iv = [0u8; 12];
        iv.copy_from_slice(&local_nonce[..12]);
        let (ct, _tag) = aes_gcm_encrypt(local_key, &iv, &[], &xored);
        key.copy_from_slice(&ct);
    } else {
        // 16 bytes exactos: ECB sin relleno no puede fallar.
        let ct = aes_ecb_encrypt(local_key, &xored, false).unwrap_or_default();
        key.copy_from_slice(&ct[..16]);
    }
    key
}

/// Lado del foco, paso 2: el cuerpo de `SESS_KEY_NEG_RESP` en claro,
/// `nonce_remoto + HMAC(local_key, nonce_local)`. Se empaqueta con
/// [`crate::device::encode_device_message`] sin encabezado de versión.
pub fn device_step2_body(local_key: &Key, local_nonce: &[u8], remote_nonce: &[u8; 16]) -> Vec<u8> {
    let mut body = remote_nonce.to_vec();
    body.extend_from_slice(&hmac_sha256(local_key, local_nonce));
    body
}

/// Lado del foco, paso 3: verifica el `SESS_KEY_NEG_FINISH` (payload ya descifrado).
pub fn device_verify_finish(local_key: &Key, remote_nonce: &[u8; 16], payload: &[u8]) -> bool {
    payload.get(..32) == Some(&hmac_sha256(local_key, remote_nonce)[..])
}
