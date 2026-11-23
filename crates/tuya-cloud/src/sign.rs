//! Firma de los pedidos a la OpenAPI de Tuya.
//!
//! Port de `tinytuya/Cloud.py::_tuyaplatform` con `new_sign_algorithm=True`:
//!
//! ```text
//! payload = client_id + [access_token] + t + stringToSign
//! stringToSign = METHOD + "\n" + sha256_hex(body) + "\n" + headers_firmados + "\n" + path_con_query
//! sign = HMAC-SHA256(payload, access_secret) en hexadecimal MAYÚSCULA
//! ```
//!
//! - `headers_firmados` queda vacío: tinytuya solo firma `Content-type`, y solo cuando manda un
//!   cuerpo (POST). Este cliente solo hace GET.
//! - El `nonce` de la documentación de Tuya es opcional y tinytuya no lo usa, así que tampoco acá.
//! - La query va **ordenada por clave y sin url-encoding** en la firma (tinytuya: "signature must
//!   be calculated before URL-encoding"), aunque la URL que se manda sí va codificada.
//!
//! `fixtures/cloud/firmas.json` tiene firmas generadas corriendo el código real de tinytuya.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

/// `sha256("")` en hexadecimal: el cuerpo de todos los GET.
pub const EMPTY_BODY_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Arma `path?k1=v1&k2=v2` con la query ordenada por clave y sin codificar, como la firma tinytuya.
pub fn sign_path(path: &str, query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return path.to_owned();
    }
    let mut sorted: Vec<&(&str, String)> = query.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let pairs: Vec<String> = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{path}?{}", pairs.join("&"))
}

/// `stringToSign` de un pedido sin encabezados firmados.
pub fn string_to_sign(method: &str, body: &[u8], path_and_query: &str) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    format!("{method}\n{body_hash}\n\n{path_and_query}")
}

/// Firma completa (hexadecimal en mayúsculas).
///
/// `token` es `None` solo para pedir el token (`GET /v1.0/token?grant_type=1`).
pub fn sign(
    access_id: &str,
    access_secret: &str,
    token: Option<&str>,
    t: &str,
    string_to_sign: &str,
) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(access_secret.as_bytes())
        .expect("HMAC acepta claves de cualquier largo");
    mac.update(access_id.as_bytes());
    if let Some(token) = token {
        mac.update(token.as_bytes());
    }
    mac.update(t.as_bytes());
    mac.update(string_to_sign.as_bytes());
    hex::encode_upper(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_del_cuerpo_vacio() {
        assert!(string_to_sign("GET", b"", "/x").contains(EMPTY_BODY_SHA256));
    }

    #[test]
    fn query_ordenada_y_sin_codificar() {
        let q = [
            ("size", "50".to_owned()),
            ("last_row_key", "A B".to_owned()),
        ];
        assert_eq!(sign_path("/v1.0/x", &q), "/v1.0/x?last_row_key=A B&size=50");
        assert_eq!(sign_path("/v1.0/x", &[]), "/v1.0/x");
    }
}
