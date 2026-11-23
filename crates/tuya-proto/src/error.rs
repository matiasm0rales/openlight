//! Errores del crate.

/// Todo lo que puede fallar al armar o desarmar mensajes Tuya.
///
/// Los textos son para el log; a la UI llegan como claves i18n desde capas más altas.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtoError {
    /// Faltan bytes para el encabezado o para el frame completo.
    #[error("faltan bytes para desarmar el mensaje")]
    NotEnoughData,
    /// El prefijo no es `0x000055AA` ni `0x00006699`.
    #[error("prefijo inválido: {0:08X}")]
    BadPrefix(u32),
    /// El encabezado declara un payload más grande que el límite (frame corrupto o desfasado).
    #[error("el encabezado declara {0} bytes de payload, más que el límite")]
    PayloadTooLarge(u32),
    /// El largo declarado no alcanza para retcode + CRC/HMAC/tag + sufijo.
    #[error("largo declarado inválido: {0}")]
    BadLength(u32),
    /// Los frames 6699 necesitan la clave.
    #[error("los frames 6699 necesitan la clave")]
    MissingKey,
    /// Cifrar en GCM necesita un IV de 12 bytes (se inyecta, nunca se genera acá).
    #[error("falta el IV de 12 bytes para GCM")]
    MissingIv,
    /// Falló la autenticación GCM (tag inválido) de un frame 6699.
    #[error("tag GCM inválido: frame rechazado")]
    AuthFailed,
    /// La clave no mide 16 bytes (o tiene caracteres fuera de latin-1).
    #[error("la clave tiene que medir 16 bytes")]
    BadKey,
    /// El texto cifrado no mide un múltiplo de 16.
    #[error("largo de texto cifrado inválido: {0}")]
    BadCiphertextLength(usize),
    /// El relleno PKCS7 es inválido.
    #[error("relleno PKCS7 inválido")]
    BadPadding,
    /// Base64 inválido (payloads de 3.1).
    #[error("base64 inválido")]
    BadBase64,
    /// El payload no es UTF-8 ni JSON válido.
    #[error("JSON inválido: {0}")]
    BadJson(String),
    /// El payload no tiene la forma esperada (`ERR_PAYLOAD` de tinytuya).
    #[error("payload inesperado")]
    UnexpectedPayload,
    /// El foco respondió `data unvalid`: hay que pasar a `dev_type = device22` y reintentar.
    #[error("el foco respondió 'data unvalid' (device22)")]
    DataUnvalid,
    /// Falló la negociación de la clave de sesión (3.4/3.5).
    #[error("falló la negociación de la clave de sesión: {0}")]
    Negotiation(&'static str),
}

/// Resultado con [`ProtoError`].
pub type Result<T, E = ProtoError> = core::result::Result<T, E>;
