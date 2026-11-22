//! Focos: identidad, credenciales, capacidades, salud del enlace y el trait [`LightDevice`].

use std::collections::BTreeMap;
use std::fmt;
use std::net::IpAddr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Secret;

/// Id de dispositivo de Tuya (por ejemplo `bf0123456789abcdefxyzw`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Versión del protocolo local de Tuya.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub enum ProtocolVersion {
    /// 3.1: JSON en claro para consultas, AES-ECB + MD5 para comandos.
    #[serde(rename = "3.1")]
    V31,
    /// 3.3: AES-128-ECB con la local_key y CRC32.
    #[serde(rename = "3.3")]
    V33,
    /// 3.4: clave de sesión negociada, AES-ECB y HMAC-SHA256.
    #[serde(rename = "3.4")]
    V34,
    /// 3.5: clave de sesión negociada y AES-128-GCM (prefijo `0x00006699`).
    #[serde(rename = "3.5")]
    V35,
}

impl ProtocolVersion {
    /// Todas las versiones soportadas.
    pub const ALL: [Self; 4] = [Self::V31, Self::V33, Self::V34, Self::V35];

    /// Texto de la versión, tal como lo usa Tuya (`"3.3"`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V31 => "3.1",
            Self::V33 => "3.3",
            Self::V34 => "3.4",
            Self::V35 => "3.5",
        }
    }

    /// Lee `"3.3"`, `"3.4"`, etc. (también acepta `3.3` como número, vía [`Self::from_f32`]).
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == value.trim())
    }

    /// Lee la versión como número (`devices.json` y `config.json` del script usan `3.5`).
    pub fn from_f32(value: f32) -> Option<Self> {
        Self::parse(&format!("{value:.1}"))
    }

    /// Si la versión negocia clave de sesión (3.4 y 3.5).
    pub const fn uses_session_key(self) -> bool {
        matches!(self, Self::V34 | Self::V35)
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lo necesario para hablar con un foco por la red local. Nunca cruza el IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCreds {
    /// Id del dispositivo.
    pub id: DeviceId,
    /// Clave local (16 caracteres). Va al llavero, nunca a un JSON.
    pub local_key: Secret<String>,
    /// Versión del protocolo.
    pub version: ProtocolVersion,
    /// IP fija o la última conocida. `None` = buscarla con discovery.
    pub ip: Option<IpAddr>,
}

/// Formato del DP de color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ColourFormat {
    /// Focos viejos (`colour_data`, DP 5): `"rrggbb0hhhssvv"`, 14 caracteres hex, s/v en 0–255.
    V1Hex14,
    /// Focos nuevos (`colour_data_v2`, DP 24): `"hhhhssssvvvv"`, h 0–360 y s/v 0–1000.
    V2Hsv12,
}

/// Un DP numérico con su rango (brillo, temperatura).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DpRange {
    /// Número de DP (`"22"`).
    pub dp: String,
    /// Valor mínimo que acepta el foco.
    pub min: i32,
    /// Valor máximo que acepta el foco.
    pub max: i32,
    /// Paso entre valores.
    pub step: i32,
}

/// Qué sabe hacer un foco, detectado del esquema de DPs (nunca DPs fijos en el código).
///
/// `tuya-device` (tarea F) implementa `Capabilities::from_mapping(DpMapping)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Capabilities {
    /// DP de encendido (`switch_led`).
    pub dp_switch: String,
    /// DP de modo (`work_mode`).
    pub dp_mode: Option<String>,
    /// DP de brillo del blanco (`bright_value(_v2)`).
    pub dp_bright: Option<DpRange>,
    /// DP de temperatura del blanco (`temp_value(_v2)`).
    pub dp_temp: Option<DpRange>,
    /// DP de color y su formato (`colour_data(_v2)`).
    pub dp_colour: Option<(String, ColourFormat)>,
    /// DP de control en tiempo real (`control_data`, DP 28 en el W509Z2).
    pub dp_control: Option<String>,
    /// Modos que acepta `dp_mode` (`white`, `colour`, `scene`, `music`).
    pub modes: Vec<String>,
}

impl Capabilities {
    /// Si el foco puede mostrar colores (si no, la app avisa en el paso 7 del asistente).
    pub fn supports_colour(&self) -> bool {
        self.dp_colour.is_some() || self.dp_control.is_some()
    }
}

/// Por dónde se le manda el color al foco (ajuste `send_channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SendChannel {
    /// DP de color normal (`"24"` en el script).
    Colour,
    /// DP de control en tiempo real, sin transición (`"directo"`).
    ControlDirect,
    /// DP de control en tiempo real, con transición (`"gradual"`).
    ControlGradual,
}

impl SendChannel {
    /// Identificador estable (el mismo que usa serde).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Colour => "colour",
            Self::ControlDirect => "control_direct",
            Self::ControlGradual => "control_gradual",
        }
    }

    /// Lee el nombre estable o el del script Python (`24` / `directo` / `gradual`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "colour" | "24" => Some(Self::Colour),
            "control_direct" | "directo" => Some(Self::ControlDirect),
            "control_gradual" | "gradual" => Some(Self::ControlGradual),
            _ => None,
        }
    }
}

/// Valores de DPs tal como viajan en el protocolo: `{"20": true, "21": "colour", ...}`.
pub type Dps = BTreeMap<String, serde_json::Value>;

/// Por qué se cortó la conexión con un foco. Sirve para investigar los cortes del log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum DisconnectReason {
    /// El foco cerró el socket (EOF).
    PeerClosed,
    /// Un envío o una lectura no terminaron a tiempo.
    Timeout,
    /// No llegó nada durante `dead_after_s`.
    NoResponse,
    /// El foco rechazó la clave (error 914).
    KeyRejected,
    /// Conexión rechazada (puerto cerrado, otra app ocupando el foco).
    Refused,
    /// Otro error de IO, con su descripción.
    Io(String),
}

/// Salud del enlace con un foco (evento `device://health`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct LinkHealth {
    /// Si hay una sesión abierta ahora.
    pub connected: bool,
    /// Tiempo de ida y vuelta del último latido, en milisegundos.
    pub rtt_ms: Option<f32>,
    /// Bytes recibidos desde que arrancó la sesión (satura en `u32::MAX`).
    pub bytes_rx: u32,
    /// Bytes enviados desde que arrancó la sesión (satura en `u32::MAX`).
    pub bytes_tx: u32,
    /// Reconexiones exitosas.
    pub reconnects: u32,
    /// Cortes en la última hora.
    pub disconnects_last_hour: u32,
    /// Motivo del último corte.
    pub last_disconnect: Option<DisconnectReason>,
    /// Cuándo fue el último corte (ms desde la época Unix).
    pub last_disconnect_at_ms: Option<f64>,
}

/// Errores de un foco.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeviceError {
    /// No se encontró el foco en la red.
    #[error("no se encontró el foco en la red")]
    NotFound,
    /// El foco rechazó la clave local (error 914).
    #[error("el foco rechazó la clave local (914)")]
    KeyRejected,
    /// Sin respuesta a tiempo.
    #[error("el foco no respondió a tiempo")]
    Timeout,
    /// Se perdió la conexión.
    #[error("se perdió la conexión: {0:?}")]
    Disconnected(DisconnectReason),
    /// Respuesta inválida o inesperada.
    #[error("error de protocolo: {0}")]
    Protocol(String),
    /// Error de red o de sistema.
    #[error("error de IO: {0}")]
    Io(String),
    /// El foco no tiene la capacidad pedida.
    #[error("el foco no soporta: {0}")]
    Unsupported(String),
}

/// Un foco controlable por la red local. Lo implementa `tuya-device` (y `tuya-sim` para pruebas).
#[async_trait]
pub trait LightDevice: Send {
    /// Lee todos los DPs (`DP_QUERY`).
    async fn status(&mut self) -> Result<Dps, DeviceError>;
    /// Escribe DPs. Con `wait = false` no espera la respuesta (como `nowait=True` de tinytuya).
    async fn set_dps(&mut self, dps: Dps, wait: bool) -> Result<(), DeviceError>;
    /// Manda una señal de vida.
    async fn heartbeat(&mut self) -> Result<(), DeviceError>;
    /// Salud actual del enlace.
    fn health(&self) -> LinkHealth;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_desde_el_numero_del_script() {
        assert_eq!(ProtocolVersion::from_f32(3.5), Some(ProtocolVersion::V35));
        assert_eq!(ProtocolVersion::from_f32(3.3), Some(ProtocolVersion::V33));
        assert_eq!(ProtocolVersion::from_f32(2.0), None);
        assert_eq!(
            serde_json::to_string(&ProtocolVersion::V34).unwrap(),
            "\"3.4\""
        );
    }

    #[test]
    fn canal_acepta_los_nombres_del_script() {
        assert_eq!(SendChannel::parse("24"), Some(SendChannel::Colour));
        assert_eq!(
            SendChannel::parse("directo"),
            Some(SendChannel::ControlDirect)
        );
        assert_eq!(
            SendChannel::parse("gradual"),
            Some(SendChannel::ControlGradual)
        );
        for c in [
            SendChannel::Colour,
            SendChannel::ControlDirect,
            SendChannel::ControlGradual,
        ] {
            assert_eq!(SendChannel::parse(c.as_str()), Some(c));
        }
    }
}
