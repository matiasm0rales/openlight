//! La interfaz chica entre el simulador y el framing/cripto de Tuya.
//!
//! El simulador no implementa ni el framing ni la cripto: eso es de `tuya-proto` (tarea A).
//! Todo lo que necesita está acá, en dos traits:
//!
//! - [`Suite`]: fábrica de [`Wire`] y las primitivas que usa la negociación de clave de sesión
//!   y el discovery UDP.
//! - [`Wire`]: codifica y decodifica los frames de **una** conexión (y guarda la clave de sesión
//!   cuando se negocia).
//!
//! Los mensajes que pasan por la interfaz ([`Msg`]) ya están descifrados: el simulador solo ve
//! JSON y bytes de la negociación.

use openlight_types::ProtocolVersion;

/// Comandos del protocolo local que usa el simulador.
/// Verificados contra `tinytuya/core/command_types.py`.
pub mod cmd {
    /// Inicio de la negociación de clave de sesión (nonce del cliente).
    pub const SESS_KEY_NEG_START: u32 = 3;
    /// Respuesta del foco: nonce del foco + HMAC del nonce del cliente.
    pub const SESS_KEY_NEG_RESP: u32 = 4;
    /// Cierre de la negociación: HMAC del nonce del foco.
    pub const SESS_KEY_NEG_FINISH: u32 = 5;
    /// Escribir DPs (3.1 y 3.3).
    pub const CONTROL: u32 = 7;
    /// Push de estado del foco.
    pub const STATUS: u32 = 8;
    /// Latido.
    pub const HEART_BEAT: u32 = 9;
    /// Consultar DPs (3.1 y 3.3).
    pub const DP_QUERY: u32 = 0x0a;
    /// Escribir DPs (3.4 y 3.5).
    pub const CONTROL_NEW: u32 = 0x0d;
    /// Consultar DPs (3.4 y 3.5).
    pub const DP_QUERY_NEW: u32 = 0x10;
    /// Pedir que el foco refresque algunos DPs.
    pub const UPDATEDPS: u32 = 0x12;
    /// Broadcast UDP de discovery (3.1 a 3.4).
    pub const UDP_NEW: u32 = 0x13;
    /// Broadcast UDP de discovery (3.5).
    pub const BOARDCAST_LPV34: u32 = 0x23;
    /// Pedido de la app al puerto 7000 para que los focos 3.5 se anuncien.
    pub const REQ_DEVINFO: u32 = 0x25;
}

/// Un mensaje ya descifrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    /// Número de secuencia del frame.
    pub seqno: u32,
    /// Comando (ver [`cmd`]).
    pub cmd: u32,
    /// Código de retorno. Lo llevan los frames del foco, nunca los del cliente.
    pub retcode: Option<u32>,
    /// Payload en claro, sin el encabezado de versión (`"3.3"` + 12 ceros).
    pub payload: Vec<u8>,
}

/// De qué lado de la conexión está un [`Wire`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// El foco: decodifica frames del cliente y codifica frames con retcode.
    Device,
    /// La app: el caso inverso. Lo usan las pruebas del simulador.
    Client,
}

/// Error al decodificar un frame.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// CRC, HMAC, tag de GCM o padding inválidos: la otra punta usa otra clave.
    #[error("autenticación inválida (¿otra clave?)")]
    BadAuth,
    /// Frame roto que no se puede resincronizar.
    #[error("frame inválido: {0}")]
    Malformed(String),
}

/// Framing y cripto de una conexión.
pub trait Wire: Send {
    /// Saca de `buf` el primer frame completo y lo devuelve descifrado.
    ///
    /// `Ok(None)` si todavía faltan bytes. Consume los bytes del frame (y la basura que haya
    /// antes del prefijo).
    fn decode(&mut self, buf: &mut Vec<u8>) -> Result<Option<Msg>, WireError>;

    /// Arma un frame listo para mandar. `with_header` agrega el encabezado de versión
    /// (`"3.x"` + 12 ceros) donde el protocolo lo usa.
    fn encode(&mut self, msg: &Msg, with_header: bool) -> Vec<u8>;

    /// Cambia la clave con la que se cifra y autentica (la de sesión, en 3.4 y 3.5).
    fn set_key(&mut self, key: [u8; 16]);
}

/// Fábrica de [`Wire`] y primitivas de cripto que el simulador necesita.
pub trait Suite: Send + Sync + 'static {
    /// Un [`Wire`] nuevo para una conexión.
    fn wire(&self, version: ProtocolVersion, key: [u8; 16], role: Role) -> Box<dyn Wire>;

    /// HMAC-SHA256.
    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> [u8; 32];

    /// Clave de sesión de 3.4/3.5 a partir de los dos nonces.
    fn session_key(
        &self,
        version: ProtocolVersion,
        local_key: &[u8; 16],
        client_nonce: &[u8; 16],
        device_nonce: &[u8; 16],
    ) -> [u8; 16];

    /// Arma un broadcast UDP de discovery con `payload` (JSON) para la versión dada.
    fn udp_encode(&self, version: ProtocolVersion, payload: &[u8]) -> Vec<u8>;

    /// Decodifica un paquete UDP (broadcast o `REQ_DEVINFO`). Devuelve el comando y el JSON.
    fn udp_decode(&self, frame: &[u8]) -> Option<(u32, Vec<u8>)>;
}
