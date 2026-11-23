//! Elección del backend en tiempo de ejecución. Código puro: recibe el entorno y el resultado de
//! sondear el portal, así se prueba sin sesión gráfica.

use openlight_types::CaptureError;

/// Variable de entorno para forzar un backend (`x11` o `wayland`), útil para diagnosticar.
pub(crate) const FORCE_VAR: &str = "OPENLIGHT_CAPTURE";

/// Backend de captura elegido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxBackend {
    /// Sesión X11: RandR para los monitores y XRender para reducir el cuadro en el servidor.
    X11,
    /// Sesión Wayland sin portal ScreenCast, por XWayland. Solo se ven las ventanas X11: las
    /// ventanas nativas de Wayland salen negras. Es un último recurso.
    XWayland,
    /// Sesión Wayland con el portal ScreenCast y PipeWire.
    WaylandPortal,
}

/// Lo que importa del entorno del proceso.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Env {
    /// `XDG_SESSION_TYPE`.
    pub session_type: Option<String>,
    /// `WAYLAND_DISPLAY`.
    pub wayland_display: Option<String>,
    /// `DISPLAY`.
    pub display: Option<String>,
    /// [`FORCE_VAR`].
    pub forced: Option<String>,
}

impl Env {
    /// Lee el entorno del proceso. Las variables vacías cuentan como ausentes.
    pub(crate) fn from_process() -> Self {
        let get = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Self {
            session_type: get("XDG_SESSION_TYPE"),
            wayland_display: get("WAYLAND_DISPLAY"),
            display: get("DISPLAY"),
            forced: get(FORCE_VAR),
        }
    }

    fn is_wayland(&self) -> bool {
        self.wayland_display.is_some()
            || self
                .session_type
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("wayland"))
    }
}

/// Resultado de sondear el portal ScreenCast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PortalStatus {
    /// Está y ofrece capturar monitores.
    Available,
    /// No está o no sirve; el texto dice por qué (para el log).
    Missing(String),
}

/// Elige el backend. `probe_portal` solo se llama si hace falta.
pub(crate) fn choose_backend(
    env: &Env,
    probe_portal: impl FnOnce() -> PortalStatus,
) -> Result<LinuxBackend, CaptureError> {
    if let Some(forced) = env.forced.as_deref() {
        return match forced.to_ascii_lowercase().as_str() {
            "x11" | "xwayland" if env.display.is_some() => Ok(if env.is_wayland() {
                LinuxBackend::XWayland
            } else {
                LinuxBackend::X11
            }),
            "x11" | "xwayland" => Err(CaptureError::Unsupported(format!(
                "{FORCE_VAR}={forced} pero no hay DISPLAY"
            ))),
            "wayland" | "portal" => match probe_portal() {
                PortalStatus::Available => Ok(LinuxBackend::WaylandPortal),
                PortalStatus::Missing(why) => Err(CaptureError::Unsupported(format!(
                    "{FORCE_VAR}={forced} pero el portal ScreenCast no está: {why}"
                ))),
            },
            other => Err(CaptureError::Unsupported(format!(
                "{FORCE_VAR}={other} no es válido (x11 o wayland)"
            ))),
        };
    }

    if env.is_wayland() {
        return match probe_portal() {
            PortalStatus::Available => Ok(LinuxBackend::WaylandPortal),
            PortalStatus::Missing(_) if env.display.is_some() => Ok(LinuxBackend::XWayland),
            PortalStatus::Missing(why) => Err(CaptureError::Unsupported(format!(
                "Wayland sin portal ScreenCast ni XWayland: {why}"
            ))),
        };
    }
    if env.display.is_some() {
        return Ok(LinuxBackend::X11);
    }
    Err(CaptureError::Unsupported(
        "no hay sesión gráfica (faltan DISPLAY y WAYLAND_DISPLAY)".into(),
    ))
}

/// Si el nombre de la salida es el de una pantalla integrada (notebook).
pub(crate) fn looks_builtin(output_name: &str) -> bool {
    let name = output_name.to_ascii_uppercase();
    ["EDP", "LVDS", "DSI"].iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(session: Option<&str>, wayland: Option<&str>, display: Option<&str>) -> Env {
        Env {
            session_type: session.map(Into::into),
            wayland_display: wayland.map(Into::into),
            display: display.map(Into::into),
            forced: None,
        }
    }

    fn portal_ok() -> PortalStatus {
        PortalStatus::Available
    }

    fn sin_portal() -> PortalStatus {
        PortalStatus::Missing("no hay xdg-desktop-portal".into())
    }

    #[test]
    fn x11_puro_no_sondea_el_portal() {
        let e = env(Some("x11"), None, Some(":0"));
        let out = choose_backend(&e, || panic!("no debería sondear el portal"));
        assert_eq!(out, Ok(LinuxBackend::X11));
    }

    #[test]
    fn wayland_con_portal_usa_el_portal() {
        let e = env(Some("wayland"), Some("wayland-0"), Some(":0"));
        assert_eq!(
            choose_backend(&e, portal_ok),
            Ok(LinuxBackend::WaylandPortal)
        );
    }

    #[test]
    fn wayland_sin_portal_cae_a_xwayland() {
        let e = env(Some("wayland"), Some("wayland-0"), Some(":0"));
        assert_eq!(choose_backend(&e, sin_portal), Ok(LinuxBackend::XWayland));
    }

    #[test]
    fn wayland_sin_portal_ni_xwayland_no_esta_soportado() {
        let e = env(Some("wayland"), Some("wayland-0"), None);
        assert!(matches!(
            choose_backend(&e, sin_portal),
            Err(CaptureError::Unsupported(_))
        ));
    }

    #[test]
    fn alcanza_con_wayland_display() {
        let e = env(None, Some("wayland-1"), None);
        assert_eq!(
            choose_backend(&e, portal_ok),
            Ok(LinuxBackend::WaylandPortal)
        );
    }

    #[test]
    fn sin_sesion_grafica() {
        assert!(matches!(
            choose_backend(&env(Some("tty"), None, None), portal_ok),
            Err(CaptureError::Unsupported(_))
        ));
    }

    #[test]
    fn se_puede_forzar() {
        let mut e = env(Some("wayland"), Some("wayland-0"), Some(":0"));
        e.forced = Some("X11".into());
        assert_eq!(
            choose_backend(&e, || panic!("no hace falta el portal")),
            Ok(LinuxBackend::XWayland)
        );
        e.forced = Some("wayland".into());
        assert!(matches!(
            choose_backend(&e, sin_portal),
            Err(CaptureError::Unsupported(_))
        ));
        e.forced = Some("cualquiera".into());
        assert!(choose_backend(&e, portal_ok).is_err());
    }

    #[test]
    fn pantallas_integradas() {
        assert!(looks_builtin("eDP-1"));
        assert!(looks_builtin("LVDS1"));
        assert!(looks_builtin("DSI-1"));
        assert!(!looks_builtin("HDMI-A-1"));
        assert!(!looks_builtin("DP-2"));
    }
}
