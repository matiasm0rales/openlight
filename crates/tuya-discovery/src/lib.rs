//! Descubrimiento de focos Tuya en la LAN (UDP 6666/6667/7000).
//!
//! Sección 3.5 de `docs/PLAN.md`, verificado contra `tinytuya/scanner.py` y
//! `tinytuya/core/udp_helper.py` (tinytuya 1.20):
//!
//! - [`scan::listen`] / [`scan::scan`]: escuchan 6666 (texto plano), 6667 (cifrado con
//!   `md5("yGAdlopoPVldABfn")`) y 7000 (3.5, 6699 + GCM), y mandan `REQ_DEVINFO` (0x25) al
//!   broadcast de cada interfaz cada 6 s.
//! - [`parse::parse_datagram`]: descifra con `tuya_proto::udp` (tarea A; este crate no tiene
//!   cripto propia) y aplica las reglas de `scanner.py`.
//! - [`match_with_cloud`]: cruza lo encontrado con los dispositivos de la nube para el paso 7
//!   del asistente.

pub mod parse;
pub mod scan;

use openlight_types::ipc::{CloudDeviceInfo, DiscoveredDevice};
use openlight_types::{CloudDevice, ErrorCode, FixAction, HelpTopic, SystemPane, UserFacingError};

pub use parse::{Datagram, parse_datagram};
pub use scan::{DiscoveryError, ScanConfig, listen, scan};

/// Cruza los dispositivos de la nube con los encontrados en la red (mismo id).
///
/// Devuelve uno por dispositivo de la nube, en el mismo orden, con `lan` completo si apareció.
/// Los que están en la red pero no en la nube se dejan afuera: son de otra cuenta.
/// `capabilities` queda en `None`: se calculan desde el mapeo de DPs (`tuya-device`).
pub fn match_with_cloud(cloud: &[CloudDevice], lan: &[DiscoveredDevice]) -> Vec<CloudDeviceInfo> {
    cloud
        .iter()
        .map(|d| CloudDeviceInfo {
            id: d.id.clone(),
            name: d.name.clone(),
            category: d.category.clone(),
            product_id: d.product_id.clone(),
            is_light: d.is_light(),
            online: d.online,
            capabilities: None,
            lan: lan.iter().find(|l| l.id == d.id).cloned(),
        })
        .collect()
}

/// Error listo para la UI.
///
/// Que no se pueda abrir ningún puerto casi siempre es un permiso: Red local en macOS o el
/// firewall en Windows.
pub fn to_user_facing(error: &DiscoveryError) -> UserFacingError {
    let base = if cfg!(target_os = "macos") {
        UserFacingError::new(ErrorCode::LocalNetworkDenied)
            .with_action(FixAction::OpenSettings(SystemPane::LocalNetwork))
    } else if cfg!(target_os = "windows") {
        UserFacingError::new(ErrorCode::FirewallBlocked)
            .with_action(FixAction::OpenSettings(SystemPane::Firewall))
    } else {
        UserFacingError::new(ErrorCode::FirewallBlocked)
            .with_action(FixAction::ShowHelp(HelpTopic::Permissions))
    };
    base.with_param("detail", error.to_string())
        .with_action(FixAction::Retry)
}

#[cfg(test)]
mod tests {
    use openlight_types::{DeviceId, ProtocolVersion, Secret};

    use super::*;

    fn nube(id: &str, category: &str) -> CloudDevice {
        CloudDevice {
            id: DeviceId(id.into()),
            name: format!("foco {id}"),
            local_key: Secret::new("k".into()),
            product_id: "p".into(),
            category: category.into(),
            model: None,
            online: true,
            cloud_ip: None,
        }
    }

    #[test]
    fn cruza_por_id() {
        let lan = vec![
            DiscoveredDevice {
                id: DeviceId("b".into()),
                ip: "192.0.2.5".into(),
                version: ProtocolVersion::V35,
                product_key: None,
            },
            DiscoveredDevice {
                id: DeviceId("ajeno".into()),
                ip: "192.0.2.6".into(),
                version: ProtocolVersion::V33,
                product_key: None,
            },
        ];
        let out = match_with_cloud(&[nube("a", "dj"), nube("b", "cz")], &lan);
        assert_eq!(out.len(), 2);
        assert!(out[0].lan.is_none() && out[0].is_light);
        assert_eq!(out[1].lan.as_ref().unwrap().ip, "192.0.2.5");
        assert!(!out[1].is_light);
    }

    #[test]
    fn error_de_puertos_tiene_accion() {
        let e = DiscoveryError::Bind(std::io::Error::other("x"));
        let u = to_user_facing(&e);
        assert!(u.actions.len() >= 2);
    }
}
