//! Errores del runtime.

use std::path::PathBuf;

use openlight_types::{
    CaptureError, DeviceError, DeviceId, ErrorCode, FixAction, HelpTopic, SystemPane,
    UserFacingError,
};
use secrets::SecretError;

use crate::locks::Holder;

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
    /// El archivo no es un respaldo que se pueda leer. El texto nunca repite su contenido.
    #[error("respaldo no válido: {0}")]
    Backup(String),
    /// El respaldo usa una versión del formato que no se entiende.
    #[error("el respaldo usa el formato {found}; esta versión lee el {supported}")]
    BackupVersion {
        /// Versión que trae.
        found: u64,
        /// La que se entiende.
        supported: u32,
    },
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
            // El candado es de esta app: nunca es culpa de otra
            CoreError::DeviceLocked { holder, .. } if holder == Holder::Session.as_str() => {
                ErrorCode::DeviceInEffect
            }
            CoreError::DeviceLocked { .. } => ErrorCode::DeviceInUse,
            CoreError::NothingToDo => ErrorCode::DeviceNotFound,
            CoreError::Capture(CaptureError::PermissionDenied) => ErrorCode::ScreenPermissionDenied,
            CoreError::Capture(CaptureError::PermissionStale) => ErrorCode::ScreenPermissionStale,
            CoreError::Capture(CaptureError::DisplayNotFound(_)) => ErrorCode::NoDisplay,
            CoreError::Capture(CaptureError::Unsupported(_)) => ErrorCode::CaptureUnsupported,
            CoreError::Capture(CaptureError::Backend(_)) => ErrorCode::Internal,
            CoreError::Device(DeviceError::NotFound) => ErrorCode::DeviceNotFound,
            CoreError::Device(DeviceError::KeyRejected) => ErrorCode::DeviceKeyRejected,
            CoreError::Device(DeviceError::Timeout) => ErrorCode::DeviceTimeout,
            CoreError::Device(_) => ErrorCode::Internal,
            CoreError::Import(_) => ErrorCode::ImportInvalid,
            CoreError::Backup(_) => ErrorCode::BackupInvalid,
            CoreError::BackupVersion { .. } => ErrorCode::BackupUnsupportedVersion,
        };
        let action = match code {
            ErrorCode::ScreenPermissionDenied => {
                FixAction::OpenSettings(SystemPane::ScreenRecording)
            }
            ErrorCode::ScreenPermissionStale => FixAction::RepairScreenPermission,
            ErrorCode::CaptureUnsupported => FixAction::ShowHelp(HelpTopic::Permissions),
            ErrorCode::DeviceKeyRejected => FixAction::RenewKey,
            ErrorCode::DeviceNotFound | ErrorCode::DeviceTimeout => {
                FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting)
            }
            ErrorCode::DeviceInEffect => FixAction::RemoveFromEffect,
            _ => FixAction::Retry,
        };
        let mut visible = UserFacingError::new(code)
            .with_param("detail", error.to_string())
            .with_action(action);
        match code {
            ErrorCode::ScreenPermissionStale => {
                visible = visible.with_action(FixAction::OpenSettings(SystemPane::ScreenRecording));
            }
            // En macOS, un permiso atado a otra versión de la app se niega como uno sin dar,
            // aunque en Ajustes figure activado: ahí hace falta poder repararlo
            ErrorCode::ScreenPermissionDenied if cfg!(target_os = "macos") => {
                visible.detail = format!("{}_macos", visible.detail);
                visible = visible.with_action(FixAction::RepairScreenPermission);
            }
            _ => {}
        }
        match error {
            CoreError::DeviceLocked { device, .. } => visible.with_param("device", device.0),
            CoreError::BackupVersion { found, supported } => visible
                .with_param("found", found.to_string())
                .with_param("supported", supported.to_string()),
            _ => visible,
        }
    }
}

/// Resultado con [`CoreError`].
pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(holder: &str) -> UserFacingError {
        CoreError::DeviceLocked {
            device: DeviceId("fake1".into()),
            holder: holder.into(),
        }
        .into()
    }

    fn captura(error: CaptureError) -> UserFacingError {
        CoreError::Capture(error).into()
    }

    #[test]
    fn el_permiso_fantasma_se_ofrece_reparar_primero() {
        let visible = captura(CaptureError::PermissionStale);
        assert_eq!(visible.code, ErrorCode::ScreenPermissionStale);
        assert_eq!(
            visible.actions,
            [
                FixAction::RepairScreenPermission,
                FixAction::OpenSettings(SystemPane::ScreenRecording)
            ]
        );
    }

    // En macOS el permiso atado a otra versión se ve igual que uno sin dar (el interruptor sigue
    // prendido en Ajustes): además de abrir Ajustes hay que poder repararlo.
    #[test]
    fn sin_permiso_en_macos_tambien_se_puede_reparar() {
        let visible = captura(CaptureError::PermissionDenied);
        assert_eq!(visible.code, ErrorCode::ScreenPermissionDenied);
        let ajustes = FixAction::OpenSettings(SystemPane::ScreenRecording);
        if cfg!(target_os = "macos") {
            assert_eq!(
                visible.actions,
                [ajustes, FixAction::RepairScreenPermission]
            );
            assert_eq!(
                visible.detail,
                "error.screen_permission_denied.detail_macos"
            );
        } else {
            assert_eq!(visible.actions, [ajustes]);
            assert_eq!(visible.detail, "error.screen_permission_denied.detail");
        }
    }

    #[test]
    fn un_candado_propio_nunca_culpa_a_otra_app() {
        let effect = locked("session");
        assert_eq!(effect.code, ErrorCode::DeviceInEffect);
        assert_eq!(effect.actions[0], FixAction::RemoveFromEffect);
        assert_eq!(effect.params["device"], "fake1");
        for holder in ["calibration", "manual"] {
            let busy = locked(holder);
            assert_eq!(busy.code, ErrorCode::DeviceInUse, "{holder}");
            assert_eq!(busy.actions[0], FixAction::Retry);
            assert_eq!(busy.params["device"], "fake1");
        }
    }
}
