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

// Sistemas todavía sin implementación: sin permisos (los agregan los commits siguientes).
#[cfg(not(target_os = "macos"))]
use otros as platform;

#[cfg(not(target_os = "macos"))]
mod otros {
    use super::*;

    pub(super) fn check() -> Vec<PermissionStatus> {
        Vec::new()
    }

    pub(super) fn request_screen_recording() -> PermissionState {
        PermissionState::NotApplicable
    }
}
