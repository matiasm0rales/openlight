//! Errores visibles para el usuario: código estable, textos (claves i18n) y acciones sugeridas.
//!
//! Regla de la UI: ningún error queda sin una acción sugerida.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Código estable de un error visible. No se renombra nunca: aparece en la FAQ y en los logs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Falta el permiso de grabación de pantalla.
    ScreenPermissionDenied,
    /// macOS bloquea la red local.
    LocalNetworkDenied,
    /// El firewall bloquea el discovery UDP.
    FirewallBlocked,
    /// Wayland sin portal ScreenCast.
    CaptureUnsupported,
    /// No hay monitor para capturar.
    NoDisplay,
    /// No se encontró el foco en la red.
    DeviceNotFound,
    /// El foco rechazó la clave (914).
    DeviceKeyRejected,
    /// El foco no responde.
    DeviceTimeout,
    /// Otra app (Smart Life) ocupa la conexión local del foco.
    DeviceBusy,
    /// El foco no soporta color.
    DeviceNoColour,
    /// Firma inválida (Access Secret mal copiado).
    CloudInvalidSignature,
    /// Token inválido.
    CloudInvalidToken,
    /// Permiso denegado (Smart Life no vinculada o región equivocada).
    CloudPermissionDenied,
    /// Región equivocada, con una sugerida.
    CloudWrongRegion,
    /// Servicio no autorizado en el proyecto.
    CloudServiceNotAuthorized,
    /// Venció la prueba de IoT Core.
    CloudTrialExpired,
    /// Sin conexión a internet o la API no responde.
    CloudNetwork,
    /// Falta guardar el Access ID y el Access Secret (paso 6 del asistente).
    CloudNoCredentials,
    /// Ningún foco tiene zonas: no hay qué seguir en la pantalla.
    NoZones,
    /// La carpeta a importar no tiene un setup válido.
    ImportInvalid,
    /// Ajuste fuera de rango o desconocido.
    InvalidSetting,
    /// Error de configuración o de disco.
    Storage,
    /// Comando todavía no implementado (stubs de wave 0).
    NotImplemented,
    /// Error inesperado.
    Internal,
}

impl ErrorCode {
    /// Todos los códigos (para verificar que cada uno tenga sus textos).
    pub const ALL: [Self; 24] = [
        Self::ScreenPermissionDenied,
        Self::LocalNetworkDenied,
        Self::FirewallBlocked,
        Self::CaptureUnsupported,
        Self::NoDisplay,
        Self::DeviceNotFound,
        Self::DeviceKeyRejected,
        Self::DeviceTimeout,
        Self::DeviceBusy,
        Self::DeviceNoColour,
        Self::CloudInvalidSignature,
        Self::CloudInvalidToken,
        Self::CloudPermissionDenied,
        Self::CloudWrongRegion,
        Self::CloudServiceNotAuthorized,
        Self::CloudTrialExpired,
        Self::CloudNetwork,
        Self::CloudNoCredentials,
        Self::NoZones,
        Self::ImportInvalid,
        Self::InvalidSetting,
        Self::Storage,
        Self::NotImplemented,
        Self::Internal,
    ];

    /// Identificador estable (`device_key_rejected`).
    pub fn as_str(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Clave i18n del título: `error.<código>.title`.
    pub fn title_key(self) -> String {
        format!("error.{}.title", self.as_str())
    }

    /// Clave i18n de la explicación: `error.<código>.detail`.
    pub fn detail_key(self) -> String {
        format!("error.{}.detail", self.as_str())
    }
}

/// Panel de ajustes del sistema que se puede abrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemPane {
    /// macOS: Privacidad y seguridad → Grabación de pantalla.
    ScreenRecording,
    /// macOS: Privacidad y seguridad → Red local.
    LocalNetwork,
    /// Windows: Firewall de Windows.
    Firewall,
    /// Linux: permisos del portal (Wayland).
    ScreenCastPortal,
}

/// Tema de ayuda de la guía.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HelpTopic {
    /// Crear la cuenta en iot.tuya.com.
    TuyaAccount,
    /// Crear el proyecto Cloud.
    CloudProject,
    /// Elegir el Data Center.
    DataCenter,
    /// Vincular Smart Life con QR.
    LinkSmartLife,
    /// Extender la prueba de IoT Core.
    IotCoreTrial,
    /// Diagnóstico de un foco que no responde.
    DeviceTroubleshooting,
    /// Permisos del sistema.
    Permissions,
}

/// Acción que la UI ofrece para resolver un error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FixAction {
    /// Abrir un panel de ajustes del sistema.
    OpenSettings(SystemPane),
    /// Renovar la clave local con las credenciales guardadas.
    RenewKey,
    /// Reintentar.
    Retry,
    /// Abrir una URL externa.
    OpenUrl(String),
    /// Mostrar una sección de la ayuda.
    ShowHelp(HelpTopic),
}

/// Un error listo para mostrar.
///
/// `title` y `detail` son **claves i18n** (por defecto `error.<código>.title|detail`); la UI
/// las resuelve e interpola `params` (`{ip}`, `{region}`...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UserFacingError {
    /// Código estable.
    pub code: ErrorCode,
    /// Clave i18n del título.
    pub title: String,
    /// Clave i18n de la explicación.
    pub detail: String,
    /// Valores para interpolar en los textos.
    pub params: BTreeMap<String, String>,
    /// Acciones sugeridas, la principal primero.
    pub actions: Vec<FixAction>,
}

impl UserFacingError {
    /// Error con las claves i18n por defecto de su código y sin acciones.
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            title: code.title_key(),
            detail: code.detail_key(),
            params: BTreeMap::new(),
            actions: Vec::new(),
        }
    }

    /// Agrega una acción.
    pub fn with_action(mut self, action: FixAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Agrega un valor para interpolar.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Respuesta de los comandos IPC que todavía son stubs.
    pub fn not_implemented(command: &str) -> Self {
        Self::new(ErrorCode::NotImplemented)
            .with_param("command", command)
            .with_action(FixAction::Retry)
    }
}

impl std::fmt::Display for UserFacingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code.as_str())
    }
}

impl std::error::Error for UserFacingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claves_i18n_por_defecto() {
        let e = UserFacingError::new(ErrorCode::DeviceKeyRejected).with_action(FixAction::RenewKey);
        assert_eq!(e.title, "error.device_key_rejected.title");
        assert_eq!(e.detail, "error.device_key_rejected.detail");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["actions"][0]["kind"], "renew_key");
    }
}
