//! Permisos del sistema para el paso 2 del asistente: estado en vivo y pedido.
//!
//! - **macOS**: Grabación de pantalla (`CGPreflightScreenCaptureAccess` /
//!   `CGRequestScreenCaptureAccess`) y Red local.
//! - **Windows**: aviso del firewall para el discovery UDP 6666/6667.
//! - **Linux**: en Wayland, aviso del portal ScreenCast; en X11 no hay permisos que pedir.
//!
//! Consultar el estado ([`check`]) nunca muestra diálogos del sistema ni toca nada: la UI lo
//! llama en bucle para el re-chequeo automático. Lo único que pide algo es
//! [`request_screen_recording`] (macOS), que usa `open_system_settings`.

use openlight_types::PermissionState;
use openlight_types::SystemPane;
use openlight_types::ipc::{PermissionKind, PermissionStatus};

/// Estado de cada permiso que importa en este sistema, sin mostrar diálogos.
///
/// Es sincrónica y puede tardar (en Windows consulta el firewall): desde el IPC se llama con
/// [`check_blocking`].
pub fn check() -> Vec<PermissionStatus> {
    platform::check()
}

/// [`check`] fuera del hilo del runtime async.
pub async fn check_blocking() -> Vec<PermissionStatus> {
    tauri::async_runtime::spawn_blocking(check)
        .await
        .unwrap_or_default()
}

/// Pide el permiso de Grabación de pantalla (macOS): hace que la app aparezca en la lista de
/// Ajustes del Sistema y muestra el diálogo de macOS la primera vez. Devuelve el estado
/// resultante. En los demás sistemas no hace nada.
pub fn request_screen_recording() -> PermissionState {
    platform::request_screen_recording()
}

/// Un permiso con su panel de ajustes (si lo tiene).
fn status(
    kind: PermissionKind,
    state: PermissionState,
    pane: Option<SystemPane>,
    needs_restart: bool,
) -> PermissionStatus {
    PermissionStatus {
        kind,
        state,
        pane,
        needs_restart,
    }
}

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "macos")]
mod macos {
    use std::io;
    use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

    use super::*;

    /// Si ya se pidió Grabación de pantalla en este proceso: CoreGraphics no distingue "nunca se
    /// pidió" de "denegado", así que hasta pedirlo no se puede saber cuál de los dos es.
    static SCREEN_REQUESTED: AtomicBool = AtomicBool::new(false);

    pub(super) fn check() -> Vec<PermissionStatus> {
        let granted = CGPreflightScreenCaptureAccess();
        vec![
            status(
                PermissionKind::ScreenRecording,
                screen_recording_state(granted, SCREEN_REQUESTED.load(Ordering::Relaxed)),
                Some(SystemPane::ScreenRecording),
                // macOS solo aplica el permiso a la app después de reabrirla: mientras no esté
                // concedido, la UI avisa que hay que reabrir al terminar.
                !granted,
            ),
            status(
                PermissionKind::LocalNetwork,
                local_network_state(),
                Some(SystemPane::LocalNetwork),
                false,
            ),
        ]
    }

    pub(super) fn request_screen_recording() -> PermissionState {
        SCREEN_REQUESTED.store(true, Ordering::Relaxed);
        if CGRequestScreenCaptureAccess() {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }

    /// Estado de Grabación de pantalla a partir de lo que dice CoreGraphics.
    pub(super) fn screen_recording_state(granted: bool, requested: bool) -> PermissionState {
        match (granted, requested) {
            (true, _) => PermissionState::Granted,
            (false, true) => PermissionState::Denied,
            (false, false) => PermissionState::NotDetermined,
        }
    }

    /// Red local no tiene API de consulta (TN3179): se detecta intentando mandar un datagrama
    /// vacío al broadcast de la red local, que es justo lo que hace el discovery de los focos.
    ///
    /// - Si macOS bloquea la red local, el envío falla con `EHOSTUNREACH` ("No route to host").
    /// - Si no hay red (sin Wi-Fi), falla con otro error y no se puede saber.
    /// - La primera vez, el envío dispara el diálogo de permiso del sistema.
    ///
    /// Se manda al puerto 9 (discard): ningún foco ni servicio lo escucha.
    fn local_network_state() -> PermissionState {
        classify_probe(probe_local_network())
    }

    fn probe_local_network() -> io::Result<usize> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        socket.send_to(&[], SocketAddr::from((Ipv4Addr::BROADCAST, 9)))
    }

    /// Qué significa el resultado del envío de prueba.
    pub(super) fn classify_probe(result: io::Result<usize>) -> PermissionState {
        match result {
            Ok(_) => PermissionState::Granted,
            Err(e) if e.raw_os_error() == Some(EHOSTUNREACH) => PermissionState::Denied,
            Err(_) => PermissionState::NotDetermined,
        }
    }

    /// `EHOSTUNREACH` en macOS.
    const EHOSTUNREACH: i32 = 65;

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn grabacion_de_pantalla_distingue_los_tres_estados() {
            assert_eq!(
                screen_recording_state(true, false),
                PermissionState::Granted
            );
            assert_eq!(screen_recording_state(true, true), PermissionState::Granted);
            assert_eq!(screen_recording_state(false, true), PermissionState::Denied);
            assert_eq!(
                screen_recording_state(false, false),
                PermissionState::NotDetermined
            );
        }

        #[test]
        fn red_local_bloqueada_es_denegada() {
            let bloqueada = Err(io::Error::from_raw_os_error(EHOSTUNREACH));
            assert_eq!(classify_probe(bloqueada), PermissionState::Denied);
            assert_eq!(classify_probe(Ok(0)), PermissionState::Granted);
        }

        #[test]
        fn sin_red_no_se_sabe() {
            // ENETUNREACH (51): no hay red, no es un bloqueo de macOS.
            let sin_red = Err(io::Error::from_raw_os_error(51));
            assert_eq!(classify_probe(sin_red), PermissionState::NotDetermined);
        }

        #[test]
        fn check_devuelve_los_dos_permisos_de_macos() {
            let kinds: Vec<_> = check().iter().map(|s| s.kind).collect();
            assert_eq!(
                kinds,
                [
                    PermissionKind::ScreenRecording,
                    PermissionKind::LocalNetwork
                ]
            );
        }
    }
}

#[cfg(target_os = "windows")]
use win as platform;

/// Windows: el firewall. Windows pregunta "¿permitir el acceso?" la primera vez que una app abre
/// un socket UDP de entrada (el discovery de los focos escucha en 6666 y 6667); si se cancela,
/// crea una regla de bloqueo para la app y los focos dejan de aparecer sin ningún aviso.
#[cfg(any(target_os = "windows", test))]
mod windows_firewall {
    use super::*;

    /// Qué dicen las reglas de entrada del firewall sobre este ejecutable.
    ///
    /// `json` es la salida de `ConvertTo-Json` sobre `Select-Object Enabled, Action`: una lista de
    /// objetos. PowerShell escribe los enums como número (`Action`: 2 = Allow, 4 = Block;
    /// `Enabled`: 1 = True) y, según la versión o el idioma del sistema, a veces como texto. Se
    /// aceptan las dos formas para no depender del idioma.
    ///
    /// Una regla de bloqueo activa gana sobre una de permiso (el firewall hace lo mismo).
    pub(super) fn state_from_rules(json: &str) -> PermissionState {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            return PermissionState::NotDetermined;
        };
        let rules = match value {
            serde_json::Value::Array(rules) => rules,
            single @ serde_json::Value::Object(_) => vec![single],
            _ => Vec::new(),
        };
        let mut allowed = false;
        for rule in &rules {
            if !is_enabled(&rule["Enabled"]) {
                continue;
            }
            match action(&rule["Action"]) {
                Some(Action::Block) => return PermissionState::Denied,
                Some(Action::Allow) => allowed = true,
                None => {}
            }
        }
        if allowed {
            PermissionState::Granted
        } else {
            PermissionState::NotDetermined
        }
    }

    enum Action {
        Allow,
        Block,
    }

    fn is_enabled(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Number(n) => n.as_i64() == Some(1),
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::String(s) => s.eq_ignore_ascii_case("true") || s == "1",
            _ => false,
        }
    }

    fn action(v: &serde_json::Value) -> Option<Action> {
        match v {
            serde_json::Value::Number(n) => match n.as_i64() {
                Some(2) => Some(Action::Allow),
                Some(4) => Some(Action::Block),
                _ => None,
            },
            serde_json::Value::String(s) if s.eq_ignore_ascii_case("allow") => Some(Action::Allow),
            serde_json::Value::String(s) if s.eq_ignore_ascii_case("block") => Some(Action::Block),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn sin_reglas_no_se_sabe() {
            assert_eq!(state_from_rules("[]"), PermissionState::NotDetermined);
            assert_eq!(state_from_rules(""), PermissionState::NotDetermined);
            assert_eq!(state_from_rules("basura"), PermissionState::NotDetermined);
        }

        #[test]
        fn regla_de_permiso_activa_es_concedido() {
            let json = r#"[{"Enabled":1,"Action":2},{"Enabled":1,"Action":2}]"#;
            assert_eq!(state_from_rules(json), PermissionState::Granted);
        }

        #[test]
        fn regla_de_bloqueo_activa_gana() {
            let json = r#"[{"Enabled":1,"Action":2},{"Enabled":1,"Action":4}]"#;
            assert_eq!(state_from_rules(json), PermissionState::Denied);
        }

        #[test]
        fn reglas_desactivadas_no_cuentan() {
            let json = r#"[{"Enabled":0,"Action":4},{"Enabled":1,"Action":2}]"#;
            assert_eq!(state_from_rules(json), PermissionState::Granted);
            let json = r#"[{"Enabled":0,"Action":4}]"#;
            assert_eq!(state_from_rules(json), PermissionState::NotDetermined);
        }

        #[test]
        fn acepta_un_solo_objeto_y_enums_como_texto() {
            let un_objeto = r#"{"Enabled":1,"Action":4}"#;
            assert_eq!(state_from_rules(un_objeto), PermissionState::Denied);
            let texto = r#"[{"Enabled":"True","Action":"Allow"}]"#;
            assert_eq!(state_from_rules(texto), PermissionState::Granted);
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use super::windows_firewall::state_from_rules;
    use super::*;

    /// No abre una consola al lanzar PowerShell.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Reglas de entrada que mencionan este ejecutable. El ejecutable va por variable de entorno
    /// para no tener que escapar la ruta dentro del script.
    const SCRIPT: &str = "$ErrorActionPreference = 'SilentlyContinue'; \
        $r = Get-NetFirewallApplicationFilter -Program $env:OPENLIGHT_EXE | Get-NetFirewallRule \
        | Where-Object { $_.Direction -eq 'Inbound' } | Select-Object Enabled, Action; \
        if ($r) { ConvertTo-Json -InputObject @($r) -Compress } else { '[]' }";

    pub(super) fn check() -> Vec<PermissionStatus> {
        vec![status(
            PermissionKind::Firewall,
            firewall_state(),
            Some(SystemPane::Firewall),
            false,
        )]
    }

    pub(super) fn request_screen_recording() -> PermissionState {
        PermissionState::NotApplicable
    }

    /// Consulta el firewall con PowerShell. Si no se puede (sin PowerShell, sin permisos), el
    /// estado queda en "no se sabe" y la UI deja el aviso con el botón para abrir el firewall.
    fn firewall_state() -> PermissionState {
        let Ok(exe) = std::env::current_exe() else {
            return PermissionState::NotDetermined;
        };
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("OPENLIGHT_EXE", exe)
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match out {
            Ok(out) if out.status.success() => {
                state_from_rules(String::from_utf8_lossy(&out.stdout).trim())
            }
            _ => PermissionState::NotDetermined,
        }
    }
}

#[cfg(target_os = "linux")]
use linux as platform;

/// Linux: en X11 no hay permisos que pedir. En Wayland, la captura pasa por el portal
/// ScreenCast: el sistema muestra su propio selector de pantalla la primera vez y, si el
/// escritorio lo permite, recuerda la elección.
#[cfg(any(target_os = "linux", test))]
mod linux_session {
    /// Si la sesión gráfica es Wayland, según `XDG_SESSION_TYPE` y `WAYLAND_DISPLAY`.
    ///
    /// `XDG_SESSION_TYPE` manda; sin él, hay Wayland si existe el socket `WAYLAND_DISPLAY`.
    pub(super) fn is_wayland(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
        match session_type {
            Some(t) if t.eq_ignore_ascii_case("wayland") => true,
            Some(t) if !t.is_empty() && !t.eq_ignore_ascii_case("tty") => false,
            _ => wayland_display.is_some_and(|d| !d.is_empty()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn xdg_session_type_manda() {
            assert!(is_wayland(Some("wayland"), None));
            assert!(is_wayland(Some("Wayland"), Some("wayland-0")));
            assert!(!is_wayland(Some("x11"), Some("wayland-0")));
        }

        #[test]
        fn sin_tipo_de_sesion_mira_wayland_display() {
            assert!(is_wayland(None, Some("wayland-0")));
            assert!(is_wayland(Some("tty"), Some("wayland-1")));
            assert!(!is_wayland(None, None));
            assert!(!is_wayland(None, Some("")));
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::linux_session::is_wayland;
    use super::*;

    pub(super) fn check() -> Vec<PermissionStatus> {
        let wayland = is_wayland(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        );
        if !wayland {
            return Vec::new();
        }
        // El portal no tiene una consulta de "¿ya me dieron permiso?": el selector aparece al
        // empezar a capturar. Se informa como "todavía no se pidió" y el aviso explica qué va
        // a pasar. Cuando la captura ya corrió con éxito, el estado real lo da `capture`.
        vec![status(
            PermissionKind::ScreenCastPortal,
            PermissionState::NotDetermined,
            Some(SystemPane::ScreenCastPortal),
            false,
        )]
    }

    pub(super) fn request_screen_recording() -> PermissionState {
        PermissionState::NotApplicable
    }
}

// Otros sistemas (BSD, etc.): sin permisos que informar.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use otros as platform;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod otros {
    use super::*;

    pub(super) fn check() -> Vec<PermissionStatus> {
        Vec::new()
    }

    pub(super) fn request_screen_recording() -> PermissionState {
        PermissionState::NotApplicable
    }
}
