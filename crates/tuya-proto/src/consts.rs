//! Constantes del protocolo: comandos, prefijos, sufijos y encabezados de versión.
//!
//! Copiadas de `tinytuya/core/command_types.py`, `header.py` y `const.py` (tinytuya 1.20).

use openlight_types::ProtocolVersion;

/// Comandos Tuya (`command_types.py`).
pub mod cmd {
    /// Configuración por AP (3.0).
    pub const AP_CONFIG: u32 = 1;
    /// Descartado.
    pub const ACTIVE: u32 = 2;
    /// Negociación de clave de sesión, paso 1 (nonce local).
    pub const SESS_KEY_NEG_START: u32 = 3;
    /// Negociación, paso 2 (nonce remoto + HMAC).
    pub const SESS_KEY_NEG_RESP: u32 = 4;
    /// Negociación, paso 3 (HMAC del nonce remoto).
    pub const SESS_KEY_NEG_FINISH: u32 = 5;
    /// Desvincular.
    pub const UNBIND: u32 = 6;
    /// Mandar DPs (3.1–3.3).
    pub const CONTROL: u32 = 7;
    /// Estado que manda el foco por su cuenta.
    pub const STATUS: u32 = 8;
    /// Latido.
    pub const HEART_BEAT: u32 = 9;
    /// Consultar DPs (3.1–3.3).
    pub const DP_QUERY: u32 = 0x0a;
    /// Consultar wifi (descartado).
    pub const QUERY_WIFI: u32 = 0x0b;
    /// Vincular token.
    pub const TOKEN_BIND: u32 = 0x0c;
    /// Mandar DPs (3.4+ y device22).
    pub const CONTROL_NEW: u32 = 0x0d;
    /// Habilitar wifi.
    pub const ENABLE_WIFI: u32 = 0x0e;
    /// Info de wifi.
    pub const WIFI_INFO: u32 = 0x0f;
    /// Consultar DPs (3.4+).
    pub const DP_QUERY_NEW: u32 = 0x10;
    /// Ejecutar escena.
    pub const SCENE_EXECUTE: u32 = 0x11;
    /// Pedir que refresque DPs.
    pub const UPDATEDPS: u32 = 0x12;
    /// Broadcast UDP cifrado.
    pub const UDP_NEW: u32 = 0x13;
    /// Configuración por AP (4.0).
    pub const AP_CONFIG_NEW: u32 = 0x14;
    /// Broadcast de focos 3.4/3.5.
    pub const BOARDCAST_LPV34: u32 = 0x23;
    /// Pedido de la app al puerto 7000 para que los focos 3.5 se anuncien.
    pub const REQ_DEVINFO: u32 = 0x25;
    /// Stream extendido.
    pub const LAN_EXT_STREAM: u32 = 0x40;
}

/// Comandos que nunca llevan el encabezado de versión (`NO_PROTOCOL_HEADER_CMDS`).
pub const NO_PROTOCOL_HEADER_CMDS: [u32; 8] = [
    cmd::DP_QUERY,
    cmd::DP_QUERY_NEW,
    cmd::UPDATEDPS,
    cmd::HEART_BEAT,
    cmd::SESS_KEY_NEG_START,
    cmd::SESS_KEY_NEG_RESP,
    cmd::SESS_KEY_NEG_FINISH,
    cmd::LAN_EXT_STREAM,
];

/// Si el comando lleva el encabezado de versión (`"3.x"` + 12×0) en 3.2+.
pub fn needs_protocol_header(command: u32) -> bool {
    !NO_PROTOCOL_HEADER_CMDS.contains(&command)
}

/// Prefijo de los frames 3.1–3.4.
pub const PREFIX_55AA: u32 = 0x0000_55AA;
/// Sufijo de los frames 3.1–3.4.
pub const SUFFIX_55AA: u32 = 0x0000_AA55;
/// Prefijo de los frames 3.5.
pub const PREFIX_6699: u32 = 0x0000_6699;
/// Sufijo de los frames 3.5.
pub const SUFFIX_6699: u32 = 0x0000_9966;
/// [`PREFIX_55AA`] en bytes.
pub const PREFIX_55AA_BIN: [u8; 4] = PREFIX_55AA.to_be_bytes();
/// [`SUFFIX_55AA`] en bytes.
pub const SUFFIX_55AA_BIN: [u8; 4] = SUFFIX_55AA.to_be_bytes();
/// [`PREFIX_6699`] en bytes.
pub const PREFIX_6699_BIN: [u8; 4] = PREFIX_6699.to_be_bytes();
/// [`SUFFIX_6699`] en bytes.
pub const SUFFIX_6699_BIN: [u8; 4] = SUFFIX_6699.to_be_bytes();

/// Encabezado 55AA: prefijo, seq, cmd, len (4×u32).
pub const HEADER_LEN_55AA: usize = 16;
/// Encabezado 6699: prefijo (u32), 0 (u16), seq, cmd, len (u32).
pub const HEADER_LEN_6699: usize = 18;
/// Retcode de las respuestas (u32).
pub const RETCODE_LEN: usize = 4;
/// IV de GCM en los frames 6699.
pub const GCM_IV_LEN: usize = 12;
/// Tag de GCM en los frames 6699.
pub const GCM_TAG_LEN: usize = 16;

/// Tope de payload para detectar un stream corrupto o desfasado (`MAX_PAYLOAD_LENGTH`).
pub const MAX_PAYLOAD_LENGTH: u32 = 1440;

/// Puerto TCP local de los focos.
pub const TCP_PORT: u16 = 6668;
/// Broadcasts UDP de 3.1 (JSON en claro).
pub const UDP_PORT: u16 = 6666;
/// Broadcasts UDP cifrados de 3.3+.
pub const UDP_PORT_ENCRYPTED: u16 = 6667;
/// Broadcasts de la app y de los focos 3.5.
pub const UDP_PORT_APP: u16 = 7000;

/// Versión en bytes (`b"3.3"`).
pub fn version_bytes(version: ProtocolVersion) -> &'static [u8; 3] {
    match version {
        ProtocolVersion::V31 => b"3.1",
        ProtocolVersion::V33 => b"3.3",
        ProtocolVersion::V34 => b"3.4",
        ProtocolVersion::V35 => b"3.5",
    }
}

/// Encabezado de versión: `"3.x"` + 12 bytes en cero (`version_header` de tinytuya).
pub fn version_header(version: ProtocolVersion) -> [u8; 15] {
    let mut header = [0u8; 15];
    header[..3].copy_from_slice(version_bytes(version));
    header
}
