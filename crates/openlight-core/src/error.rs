//! Errores del runtime.

use std::path::PathBuf;

use openlight_types::{
    CaptureError, DeviceError, DeviceId, ErrorCode, FixAction, HelpTopic, SystemPane,
    UserFacingError,
};
use secrets::SecretError;

/// Todo lo que puede salir mal en `openlight-core`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoreError {
    /// No se pudo leer o escribir un archivo.
    #[error("error de disco en {path}: {detail}")]
    Io {
        /// Archivo o carpeta.
        path: PathBuf,
        /// Descripción del error del sistema.
        detail: String,
    },
    /// Un archivo no es el JSON que se esperaba.
    #[error("{path} no es válido: {detail}")]
    Json {
        /// Archivo.
        path: PathBuf,
        /// Descripción del error de JSON.
        detail: String,
    },
    /// El archivo lo escribió una versión más nueva de OpenLight.
    #[error(
        "{file} es de una versión más nueva ({found}); esta versión entiende hasta la {supported}"
    )]
    UnsupportedVersion {
        /// Nombre del archivo.
        file: String,
        /// Versión que trae.
        found: u32,
        /// Última versión que se entiende.
        supported: u32,
    },
    /// Un dato no pasa la validación (id de foco con caracteres raros, ajuste fuera de rango...).
    #[error("dato inválido: {0}")]
    Invalid(String),
    /// Otra tarea (la sesión o la calibración) tiene el foco.
    #[error("el foco {device} lo está usando «{holder}»")]
    DeviceLocked {
        /// Foco.
        device: DeviceId,
        /// Quién lo tiene.
        holder: String,
    },
    /// La sesión no tiene focos con zonas para seguir.
    #[error("no hay focos con zonas para arrancar")]
    NothingToDo,
    /// Falló el llavero.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// Falló la captura de pantalla.
    #[error(transparent)]
    Capture(#[from] CaptureError),
    /// Falló un foco.
    #[error(transparent)]
    Device(#[from] DeviceError),
    /// La carpeta a importar no tiene un setup válido.
    #[error("no se puede importar: {0}")]
    Import(String),
}

impl CoreError {
    /// Un error de disco sobre `path`.
    pub(crate) fn io(path: impl Into<PathBuf>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            detail: error.to_string(),
        }
    }

    /// Un error de JSON sobre `path`.
    pub(crate) fn json(path: impl Into<PathBuf>, error: &serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            detail: error.to_string(),
        }
    }
}

impl From<CoreError> for UserFacingError {
    fn from(error: CoreError) -> Self {
        let code = match &error {
            CoreError::Io { .. }
            | CoreError::Json { .. }
            | CoreError::UnsupportedVersion { .. }
            | CoreError::Secret(_) => ErrorCode::Storage,
            CoreError::Invalid(_) => ErrorCode::InvalidSetting,
            CoreError::DeviceLocked { .. } => ErrorCode::DeviceBusy,
            CoreError::NothingToDo => ErrorCode::DeviceNotFound,
            CoreError::Capture(CaptureError::PermissionDenied) => ErrorCode::ScreenPermissionDenied,
            CoreError::Capture(CaptureError::DisplayNotFound(_)) => ErrorCode::NoDisplay,
            CoreError::Capture(CaptureError::Unsupported(_)) => ErrorCode::CaptureUnsupported,
            CoreError::Capture(CaptureError::Backend(_)) => ErrorCode::Internal,
            CoreError::Device(DeviceError::NotFound) => ErrorCode::DeviceNotFound,
            CoreError::Device(DeviceError::KeyRejected) => ErrorCode::DeviceKeyRejected,
            CoreError::Device(DeviceError::Timeout) => ErrorCode::DeviceTimeout,
            CoreError::Device(_) => ErrorCode::Internal,
            CoreError::Import(_) => ErrorCode::ImportInvalid,
        };
        let action = match code {
            ErrorCode::ScreenPermissionDenied => {
                FixAction::OpenSettings(SystemPane::ScreenRecording)
            }
            ErrorCode::CaptureUnsupported => FixAction::ShowHelp(HelpTopic::Permissions),
            ErrorCode::DeviceKeyRejected => FixAction::RenewKey,
            ErrorCode::DeviceNotFound | ErrorCode::DeviceBusy | ErrorCode::DeviceTimeout => {
                FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting)
            }
            _ => FixAction::Retry,
        };
        UserFacingError::new(code)
            .with_param("detail", error.to_string())
            .with_action(action)
    }
}

/// Resultado con [`CoreError`].
pub type Result<T> = std::result::Result<T, CoreError>;
