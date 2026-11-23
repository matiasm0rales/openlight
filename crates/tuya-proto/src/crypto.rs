//! Primitivas criptográficas, con la misma semántica que `tinytuya/core/crypto_helper.py`.
//!
//! - AES-128-ECB con relleno PKCS7 opcional. Al descifrar, el relleno se saca como lo hace
//!   `_unpad`: sin verificar los bytes salvo que se pida (`verify_padding`).
//! - AES-128-GCM con IV de 12 bytes y tag de 16, con AAD opcional.
//! - HMAC-SHA256, CRC32 y la firma MD5 de los comandos 3.1.
//!
//! Nada genera aleatoriedad: los IV y nonces se reciben siempre por parámetro.

use aes::Aes128;
use aes_gcm::aead::AeadInOut;
use aes_gcm::{Aes128Gcm, KeyInit};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ecb::cipher::block_padding::{NoPadding, Pkcs7};
use ecb::cipher::{BlockModeDecrypt, BlockModeEncrypt};
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::Sha256;

use crate::error::{ProtoError, Result};

/// Clave AES-128 (local_key, clave de sesión o clave UDP).
pub type Key = [u8; 16];

/// Convierte la local_key de texto a bytes como tinytuya (`local_key.encode("latin1")`).
///
/// # Errors
/// [`ProtoError::BadKey`] si no mide 16 bytes o tiene caracteres fuera de latin-1.
pub fn key_from_str(local_key: &str) -> Result<Key> {
    let mut key = [0u8; 16];
    let mut n = 0usize;
    for c in local_key.chars() {
        let b = u8::try_from(u32::from(c)).map_err(|_| ProtoError::BadKey)?;
        *key.get_mut(n).ok_or(ProtoError::BadKey)? = b;
        n += 1;
    }
    if n == 16 {
        Ok(key)
    } else {
        Err(ProtoError::BadKey)
    }
}

/// Agrega relleno PKCS7 a bloques de 16 (`_pad`).
pub fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let pad = 16 - data.len() % 16;
    let mut out = Vec::with_capacity(data.len() + pad);
    out.extend_from_slice(data);
    // `pad` está entre 1 y 16: entra en un u8.
    out.resize(data.len() + pad, pad as u8);
    out
}

/// Saca el relleno PKCS7 como `_unpad`: el último byte dice cuántos sacar (1–16). Con
/// `verify` también exige que todos los bytes de relleno valgan lo mismo.
///
/// # Errors
/// [`ProtoError::BadPadding`] si el dato está vacío o el relleno es inválido.
pub fn pkcs7_unpad(data: &[u8], verify: bool) -> Result<&[u8]> {
    let &last = data.last().ok_or(ProtoError::BadPadding)?;
    let pad = usize::from(last);
    if !(1..=16).contains(&pad) || pad > data.len() {
        return Err(ProtoError::BadPadding);
    }
    let (body, padding) = data.split_at(data.len() - pad);
    if verify && padding.iter().any(|&b| b != last) {
        return Err(ProtoError::BadPadding);
    }
    Ok(body)
}

/// AES-128-ECB. Con `pad` agrega PKCS7; sin `pad` el dato tiene que medir un múltiplo de 16.
///
/// # Errors
/// [`ProtoError::BadCiphertextLength`] si `pad` es `false` y el largo no es múltiplo de 16.
pub fn aes_ecb_encrypt(key: &Key, data: &[u8], pad: bool) -> Result<Vec<u8>> {
    let enc = ecb::Encryptor::<Aes128>::new(key.into());
    if pad {
        Ok(enc.encrypt_padded_vec::<Pkcs7>(data))
    } else if data.len().is_multiple_of(16) {
        Ok(enc.encrypt_padded_vec::<NoPadding>(data))
    } else {
        Err(ProtoError::BadCiphertextLength(data.len()))
    }
}

/// AES-128-ECB inverso, sacando el relleno como [`pkcs7_unpad`].
///
/// # Errors
/// [`ProtoError::BadCiphertextLength`] si el largo no es múltiplo de 16 (o es 0) y
/// [`ProtoError::BadPadding`] si el relleno es inválido.
pub fn aes_ecb_decrypt(key: &Key, data: &[u8], verify_padding: bool) -> Result<Vec<u8>> {
    let raw = aes_ecb_decrypt_raw(key, data)?;
    Ok(pkcs7_unpad(&raw, verify_padding)?.to_vec())
}

/// AES-128-ECB inverso sin tocar el relleno.
///
/// # Errors
/// [`ProtoError::BadCiphertextLength`] si el largo no es múltiplo de 16.
pub fn aes_ecb_decrypt_raw(key: &Key, data: &[u8]) -> Result<Vec<u8>> {
    if !data.len().is_multiple_of(16) {
        return Err(ProtoError::BadCiphertextLength(data.len()));
    }
    ecb::Decryptor::<Aes128>::new(key.into())
        .decrypt_padded_vec::<NoPadding>(data)
        .map_err(|_| ProtoError::BadCiphertextLength(data.len()))
}

/// AES-128-GCM: devuelve el texto cifrado (mismo largo que `plain`) y el tag de 16 bytes.
pub fn aes_gcm_encrypt(key: &Key, iv: &[u8; 12], aad: &[u8], plain: &[u8]) -> (Vec<u8>, [u8; 16]) {
    let cipher = Aes128Gcm::new(key.into());
    let mut buf = plain.to_vec();
    let tag = cipher
        .encrypt_inout_detached(iv.into(), aad, buf.as_mut_slice().into())
        // Solo falla con mensajes de más de 64 GiB.
        .expect("GCM no falla con mensajes chicos");
    (buf, tag.into())
}

/// AES-128-GCM inverso, verificando el tag.
///
/// # Errors
/// [`ProtoError::AuthFailed`] si el tag no coincide.
pub fn aes_gcm_decrypt(
    key: &Key,
    iv: &[u8; 12],
    aad: &[u8],
    ciphertext: &[u8],
    tag: &[u8; 16],
) -> Result<Vec<u8>> {
    let cipher = Aes128Gcm::new(key.into());
    let mut buf = ciphertext.to_vec();
    cipher
        .decrypt_inout_detached(iv.into(), aad, buf.as_mut_slice().into(), tag.into())
        .map_err(|_| ProtoError::AuthFailed)?;
    Ok(buf)
}

/// HMAC-SHA256.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key)
        .expect("HMAC acepta claves de cualquier largo");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// CRC32 (IEEE), como `binascii.crc32(data) & 0xFFFFFFFF`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// MD5.
pub fn md5(data: &[u8]) -> [u8; 16] {
    Md5::digest(data).into()
}

/// Base64 estándar.
pub fn base64_encode(data: &[u8]) -> Vec<u8> {
    BASE64.encode(data).into_bytes()
}

/// Base64 estándar inverso.
///
/// # Errors
/// [`ProtoError::BadBase64`] si el texto no es base64 válido.
pub fn base64_decode(data: &[u8]) -> Result<Vec<u8>> {
    BASE64.decode(data).map_err(|_| ProtoError::BadBase64)
}

/// Firma de los CONTROL de 3.1: `md5(b"data=" + b64 + b"||lpv=3.1||" + local_key)` en hexa,
/// caracteres 8 a 24 (16 caracteres).
pub fn md5_signature_v31(b64_payload: &[u8], local_key: &Key) -> [u8; 16] {
    let mut pre = Vec::with_capacity(b64_payload.len() + 32);
    pre.extend_from_slice(b"data=");
    pre.extend_from_slice(b64_payload);
    pre.extend_from_slice(b"||lpv=3.1||");
    pre.extend_from_slice(local_key);
    let digest = md5(&pre);
    let hex = to_hex(&digest);
    let mut out = [0u8; 16];
    out.copy_from_slice(&hex[8..24]);
    out
}

/// Hexa en minúsculas.
pub(crate) fn to_hex(data: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.push(DIGITS[usize::from(b >> 4)]);
        out.push(DIGITS[usize::from(b & 0x0f)]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_clave_de_texto_se_lee_en_latin1() {
        assert_eq!(
            &key_from_str("OpenLightKey0001").unwrap(),
            b"OpenLightKey0001"
        );
        assert_eq!(key_from_str("corta"), Err(ProtoError::BadKey));
        assert_eq!(key_from_str("OpenLightKey00012"), Err(ProtoError::BadKey));
        assert_eq!(key_from_str("€penLightKey0001"), Err(ProtoError::BadKey));
        assert_eq!(key_from_str("ñpenLightKey0001").unwrap()[0], 0xf1);
    }

    #[test]
    fn el_relleno_se_saca_como_unpad() {
        let key = *b"0123456789abcdef";
        for n in 0..40 {
            let data: Vec<u8> = (0..n).map(|i| i as u8).collect();
            let enc = aes_ecb_encrypt(&key, &data, true).unwrap();
            assert_eq!(enc.len() % 16, 0);
            assert_eq!(aes_ecb_decrypt(&key, &enc, true).unwrap(), data);
        }
        // Sin verificar, solo cuenta el último byte (como `_unpad` por defecto).
        assert_eq!(pkcs7_unpad(&[1, 2, 9, 2], false).unwrap(), &[1, 2]);
        assert_eq!(
            pkcs7_unpad(&[1, 2, 9, 2], true),
            Err(ProtoError::BadPadding)
        );
        assert_eq!(pkcs7_unpad(&[0; 16], false), Err(ProtoError::BadPadding));
        assert_eq!(pkcs7_unpad(&[], false), Err(ProtoError::BadPadding));
        assert_eq!(
            aes_ecb_decrypt(&key, &[0; 15], false),
            Err(ProtoError::BadCiphertextLength(15))
        );
        assert!(aes_ecb_encrypt(&key, &[0; 15], false).is_err());
    }

    #[test]
    fn gcm_ida_y_vuelta() {
        let key = *b"0123456789abcdef";
        let iv = *b"0123456789ab";
        let (ct, tag) = aes_gcm_encrypt(&key, &iv, b"aad", b"hola");
        assert_eq!(
            aes_gcm_decrypt(&key, &iv, b"aad", &ct, &tag).unwrap(),
            b"hola"
        );
        assert_eq!(
            aes_gcm_decrypt(&key, &iv, b"otro", &ct, &tag),
            Err(ProtoError::AuthFailed)
        );
    }
}
