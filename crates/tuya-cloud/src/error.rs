//! Traducción de los errores de Tuya a [`CloudError`] y de ahí a [`UserFacingError`].
//!
//! ## Fuentes de los códigos
//!
//! Todos los códigos y mensajes salen de la página oficial **Global Error Codes** de Tuya
//! (<https://developer.tuya.com/en/docs/iot/error-code?id=K989ruxx88swc>, actualizada el
//! 2024-07-22), salvo que se diga otra cosa:
//!
//! | código | mensaje de Tuya | se traduce a |
//! |---|---|---|
//! | 1001 | `secret invalid` | [`CloudError::InvalidSignature`] |
//! | 1004 | `sign invalid` | [`CloudError::InvalidSignature`] |
//! | 1005 | `clientId invalid` | [`CloudError::InvalidSignature`] (credenciales mal copiadas: el Access ID; ver nota abajo) |
//! | 1002 | `access_token is null` | [`CloudError::InvalidToken`] |
//! | 1010 | `token is expired` | [`CloudError::InvalidToken`] (antes se renueva el token una vez) |
//! | 1011, 1400 | `token invalid` | [`CloudError::InvalidToken`] (ídem; tinytuya busca `"token invalid"` en la respuesta) |
//! | 1106 | `permission deny` | [`CloudError::PermissionDenied`] (y se prueban las otras regiones) |
//! | 28841001 | `No permission` (sin plan de desarrollo) | [`CloudError::ServiceNotAuthorized`] |
//! | 28841101, 28841106 | `API is not subscribed` | [`CloudError::ServiceNotAuthorized`] |
//! | 28841105 | `Project is not authorized to use this API` | [`CloudError::ServiceNotAuthorized`] |
//! | 60009001 | `Not in the permission package` | [`CloudError::ServiceNotAuthorized`] |
//! | 28841002 | `Cloud development plan expired` | [`CloudError::TrialExpired`] |
//! | 28841003 | `Plan bill is overdue` | [`CloudError::TrialExpired`] |
//! | 28841004 | `Trial Edition quota is exhausted` | [`CloudError::TrialExpired`] |
//! | 28841102 | `API subscription expired` | [`CloudError::TrialExpired`] |
//! | 28841103 | `API bill is overdue` | [`CloudError::TrialExpired`] |
//! | 28841104 | `API quota is exhausted` | [`CloudError::TrialExpired`] |
//! | 2009 | `not support this device` | [`CloudError::Api`]; en `mapping` es un mapeo vacío, como `tinytuya/Cloud.py::getmapping` |
//! | otro | — | [`CloudError::Api`] con el código y el mensaje originales |
//!
//! Los 288410xx de "vencido / agotado / impago" se agrupan en `TrialExpired` porque en todos el
//! arreglo es el mismo: renovar o extender el plan en iot.tuya.com (el caso típico es la prueba de
//! IoT Core). `wizard.py` da la misma pista ("your Tuya IoT subscription has expired") cuando el
//! error menciona `permission` o `1010`.
//!
//! 1005 no es un token vencido: reintentar no lo arregla. Va con los errores de credenciales, cuya
//! acción principal es volver a copiarlas desde iot.tuya.com. Un código propio para el Access ID
//! necesita un cambio de contrato (`ErrorCode` y textos de la UI).
//!
//! Las fixtures `fixtures/cloud/error_*.json` reproducen estas respuestas. Cuando haya respuestas
//! reales saneadas que contradigan la tabla, manda la respuesta real.

use openlight_types::{CloudError, ErrorCode, FixAction, HelpTopic, UserFacingError};

/// Portal de Tuya donde se arreglan las credenciales, el plan y los servicios.
pub const TUYA_IOT_URL: &str = "https://iot.tuya.com";

/// Códigos de "el token ya no sirve": se renueva el token y se reintenta una vez.
pub const TOKEN_ERROR_CODES: [i64; 4] = [1002, 1010, 1011, 1400];

/// Código de "el dispositivo no tiene DPs" (ver `Cloud.py::getmapping`).
pub const NOT_SUPPORTED_DEVICE: i64 = 2009;

/// Si una respuesta fallida pide renovar el token.
pub fn is_token_error(code: i64, msg: &str) -> bool {
    // tinytuya: `if "token invalid" in response.text`.
    TOKEN_ERROR_CODES.contains(&code) || msg.contains("token invalid")
}

/// Traduce el `code`/`msg` de una respuesta con `success: false`.
pub fn classify(code: i64, msg: &str) -> CloudError {
    match code {
        1001 | 1004 | 1005 => CloudError::InvalidSignature,
        1002 | 1010 | 1011 | 1400 => CloudError::InvalidToken,
        1106 => CloudError::PermissionDenied {
            suggested_region: None,
        },
        28841001 | 28841101 | 28841105 | 28841106 | 60009001 => CloudError::ServiceNotAuthorized,
        28841002 | 28841003 | 28841004 | 28841102 | 28841103 | 28841104 => CloudError::TrialExpired,
        _ if msg.contains("token invalid") => CloudError::InvalidToken,
        _ => CloudError::Api {
            code,
            msg: msg.to_owned(),
        },
    }
}

/// Error listo para la UI, con las claves i18n de `ErrorCode` y al menos una acción.
///
/// `CloudError::PermissionDenied` con región sugerida se vuelve `cloud_wrong_region`, con el
/// parámetro `{region}` = código corto de la región (`eu`, `us-e`...).
pub fn to_user_facing(error: &CloudError) -> UserFacingError {
    let open_portal = FixAction::OpenUrl(TUYA_IOT_URL.to_owned());
    match error {
        CloudError::InvalidSignature => UserFacingError::new(ErrorCode::CloudInvalidSignature)
            .with_action(open_portal)
            .with_action(FixAction::Retry),
        CloudError::InvalidToken => UserFacingError::new(ErrorCode::CloudInvalidToken)
            .with_action(FixAction::Retry)
            .with_action(open_portal),
        CloudError::PermissionDenied {
            suggested_region: Some(region),
        } => UserFacingError::new(ErrorCode::CloudWrongRegion)
            .with_param("region", region.code())
            .with_action(FixAction::Retry)
            .with_action(FixAction::ShowHelp(HelpTopic::DataCenter)),
        CloudError::PermissionDenied {
            suggested_region: None,
        } => UserFacingError::new(ErrorCode::CloudPermissionDenied)
            .with_action(FixAction::ShowHelp(HelpTopic::LinkSmartLife))
            .with_action(FixAction::ShowHelp(HelpTopic::DataCenter))
            .with_action(FixAction::Retry),
        CloudError::ServiceNotAuthorized => {
            UserFacingError::new(ErrorCode::CloudServiceNotAuthorized)
                .with_action(FixAction::ShowHelp(HelpTopic::CloudProject))
                .with_action(open_portal)
                .with_action(FixAction::Retry)
        }
        CloudError::TrialExpired => UserFacingError::new(ErrorCode::CloudTrialExpired)
            .with_action(FixAction::ShowHelp(HelpTopic::IotCoreTrial))
            .with_action(open_portal)
            .with_action(FixAction::Retry),
        CloudError::Network(_) => {
            UserFacingError::new(ErrorCode::CloudNetwork).with_action(FixAction::Retry)
        }
        CloudError::Api { code, msg } => UserFacingError::new(ErrorCode::Internal)
            .with_param("code", code.to_string())
            .with_param("msg", msg.clone())
            .with_action(FixAction::Retry),
        CloudError::Decode(detail) => UserFacingError::new(ErrorCode::Internal)
            .with_param("msg", detail.clone())
            .with_action(FixAction::Retry),
    }
}

#[cfg(test)]
mod tests {
    use openlight_types::Region;

    use super::*;

    #[test]
    fn codigos_oficiales() {
        assert_eq!(classify(1004, "sign invalid"), CloudError::InvalidSignature);
        assert_eq!(
            classify(1001, "secret invalid"),
            CloudError::InvalidSignature
        );
        // 1005 es un Access ID mal copiado, no un token vencido.
        assert_eq!(
            classify(1005, "clientId invalid"),
            CloudError::InvalidSignature
        );
        assert_eq!(classify(1010, "token is expired"), CloudError::InvalidToken);
        assert_eq!(
            classify(1106, "permission deny"),
            CloudError::PermissionDenied {
                suggested_region: None
            }
        );
        assert_eq!(classify(28841105, "x"), CloudError::ServiceNotAuthorized);
        assert_eq!(classify(28841002, "x"), CloudError::TrialExpired);
        assert_eq!(
            classify(1013, "request time is invalid"),
            CloudError::Api {
                code: 1013,
                msg: "request time is invalid".into()
            }
        );
    }

    #[test]
    fn token_invalido_por_texto_como_tinytuya() {
        assert!(is_token_error(9999, "token invalid"));
        assert!(is_token_error(1010, ""));
        assert!(!is_token_error(1004, "sign invalid"));
    }

    #[test]
    fn todo_error_visible_tiene_accion() {
        let todos = [
            CloudError::InvalidSignature,
            CloudError::InvalidToken,
            CloudError::PermissionDenied {
                suggested_region: None,
            },
            CloudError::PermissionDenied {
                suggested_region: Some(Region::Eu),
            },
            CloudError::ServiceNotAuthorized,
            CloudError::TrialExpired,
            CloudError::Network("x".into()),
            CloudError::Api {
                code: 1,
                msg: "x".into(),
            },
            CloudError::Decode("x".into()),
        ];
        for e in &todos {
            let u = to_user_facing(e);
            assert!(!u.actions.is_empty(), "{e:?} sin acción");
            assert_eq!(u.title, u.code.title_key());
        }
    }

    #[test]
    fn region_sugerida() {
        let u = to_user_facing(&CloudError::PermissionDenied {
            suggested_region: Some(Region::EuWest),
        });
        assert_eq!(u.code, ErrorCode::CloudWrongRegion);
        assert_eq!(u.params["region"], "eu-w");
    }
}
