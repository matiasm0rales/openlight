//! Traducción de los errores de cada capa a [`UserFacingError`]: cada uno lleva su código estable,
//! las claves i18n de ese código y al menos una acción sugerida. Los textos viven en
//! `app/ui/src/lib/i18n/{es,en}.json`, nunca acá.

use openlight_types::{
    CaptureError, DeviceError, DeviceId, DisconnectReason, ErrorCode, FixAction, HelpTopic,
    UserFacingError,
};

/// Resultado de todos los comandos: los errores siempre llegan listos para mostrar.
pub type CmdResult<T> = Result<T, UserFacingError>;

/// Un error de un foco, con su id para que la UI diga de cuál se trata.
pub fn device_error(device: &DeviceId, error: &DeviceError) -> UserFacingError {
    let troubleshooting = FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting);
    let base = match error {
        DeviceError::NotFound => UserFacingError::new(ErrorCode::DeviceNotFound)
            .with_action(FixAction::Retry)
            .with_action(troubleshooting),
        DeviceError::KeyRejected => UserFacingError::new(ErrorCode::DeviceKeyRejected)
            .with_action(FixAction::RenewKey)
            .with_action(troubleshooting),
        DeviceError::Disconnected(DisconnectReason::Refused) => {
            UserFacingError::new(ErrorCode::DeviceBusy)
                .with_action(troubleshooting)
                .with_action(FixAction::Retry)
        }
        DeviceError::Disconnected(DisconnectReason::KeyRejected) => {
            UserFacingError::new(ErrorCode::DeviceKeyRejected)
                .with_action(FixAction::RenewKey)
                .with_action(troubleshooting)
        }
        DeviceError::Timeout | DeviceError::Disconnected(_) => {
            UserFacingError::new(ErrorCode::DeviceTimeout)
                .with_action(FixAction::Retry)
                .with_action(troubleshooting)
        }
        DeviceError::Unsupported(_) => {
            UserFacingError::new(ErrorCode::DeviceNoColour).with_action(troubleshooting)
        }
        DeviceError::Protocol(_) | DeviceError::Io(_) => {
            UserFacingError::new(ErrorCode::Internal).with_action(FixAction::Retry)
        }
    };
    base.with_param("device", device.0.clone())
        .with_param("detail", error.to_string())
}

/// Un error de captura (permiso, monitor que no está, Wayland sin portal).
pub fn capture_error(error: &CaptureError) -> UserFacingError {
    openlight_core::CoreError::Capture(error.clone()).into()
}

/// Un error inesperado de la app: va con "reintentar" y el detalle técnico para el log.
pub fn internal(detail: impl Into<String>) -> UserFacingError {
    UserFacingError::new(ErrorCode::Internal)
        .with_param("detail", detail.into())
        .with_action(FixAction::Retry)
}

/// Faltan las credenciales de la nube.
pub fn no_credentials() -> UserFacingError {
    UserFacingError::new(ErrorCode::CloudNoCredentials)
        .with_action(FixAction::ShowHelp(HelpTopic::CloudProject))
}

/// Un dato que la UI mandó y no es válido (id de foco desconocido, IP mal escrita...).
pub fn invalid(what: &str, detail: impl Into<String>) -> UserFacingError {
    UserFacingError::new(ErrorCode::InvalidSetting)
        .with_param("setting", what)
        .with_param("detail", detail.into())
        .with_action(FixAction::Retry)
}

/// El foco que pide la UI no está configurado.
pub fn unknown_device(device: &DeviceId) -> UserFacingError {
    UserFacingError::new(ErrorCode::DeviceNotFound)
        .with_param("device", device.0.clone())
        .with_action(FixAction::ShowHelp(HelpTopic::DeviceTroubleshooting))
        .with_action(FixAction::Retry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_error_de_un_foco_trae_accion_y_dice_cual_foco() {
        let id = DeviceId("fake1".into());
        for error in [
            DeviceError::NotFound,
            DeviceError::KeyRejected,
            DeviceError::Timeout,
            DeviceError::Disconnected(DisconnectReason::Refused),
            DeviceError::Disconnected(DisconnectReason::PeerClosed),
            DeviceError::Protocol("x".into()),
            DeviceError::Io("x".into()),
            DeviceError::Unsupported("x".into()),
        ] {
            let visible = device_error(&id, &error);
            assert!(!visible.actions.is_empty(), "{error:?} sin acción");
            assert_eq!(visible.params["device"], "fake1");
        }
    }

    #[test]
    fn la_clave_rechazada_ofrece_renovarla_primero() {
        let visible = device_error(&DeviceId("a".into()), &DeviceError::KeyRejected);
        assert_eq!(visible.code, ErrorCode::DeviceKeyRejected);
        assert_eq!(visible.actions[0], FixAction::RenewKey);
    }

    #[test]
    fn los_detalles_tecnicos_no_traen_secretos() {
        // El detalle es el `Display` del error: ninguno de los de `DeviceError` lleva claves.
        let visible = device_error(&DeviceId("a".into()), &DeviceError::Io("fallo".into()));
        assert!(visible.params["detail"].contains("fallo"));
    }
}
