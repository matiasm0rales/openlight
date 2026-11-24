//! "Abrir al iniciar sesión" (paso 11 del asistente y ajuste del panel).
//!
//! Usa `tauri-plugin-autostart`: LaunchAgent en macOS, clave `Run` del registro en Windows y
//! archivo `.desktop` en `~/.config/autostart` en Linux. La app arranca con [`LAUNCH_ARG`] y, si
//! lo recibe, queda en el tray sin abrir la ventana ([`started_by_autostart`]).
//!
//! La UI usa las funciones JS del plugin (`enable`, `disable`, `isEnabled`); estas funciones son
//! para el backend (por ejemplo, el ajuste de "abrir al iniciar" y el menú del tray).

use openlight_types::{ErrorCode, UserFacingError};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};
#[cfg(target_os = "macos")]
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

/// Argumento con el que el sistema lanza la app al iniciar sesión.
pub const LAUNCH_ARG: &str = "--minimized";

/// El plugin, listo para `Builder::plugin`.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    let builder = tauri_plugin_autostart::Builder::new().arg(LAUNCH_ARG);
    // Solo existe en macOS (en los demás sistemas el plugin no tiene alternativas).
    #[cfg(target_os = "macos")]
    let builder = builder.macos_launcher(MacosLauncher::LaunchAgent);
    builder.build()
}

/// Si está activado "abrir al iniciar sesión".
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, UserFacingError> {
    app.autolaunch().is_enabled().map_err(internal)
}

/// Activa o desactiva "abrir al iniciar sesión". Pedir lo que ya está puesto no hace nada.
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), UserFacingError> {
    if is_enabled(app)? == enabled {
        return Ok(());
    }
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(internal)
}

/// Si la app arrancó con [`LAUNCH_ARG`]: abrió sola al iniciar sesión y no tiene que mostrar la
/// ventana.
pub fn started_by_autostart() -> bool {
    launched_with(std::env::args())
}

/// Si algún argumento es [`LAUNCH_ARG`] (el primero es el ejecutable y no cuenta).
pub fn launched_with<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().skip(1).any(|a| a.as_ref() == LAUNCH_ARG)
}

fn internal(e: tauri_plugin_autostart::Error) -> UserFacingError {
    UserFacingError::new(ErrorCode::Internal).with_param("detail", e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_el_argumento_de_inicio() {
        assert!(launched_with(["openlight", "--minimized"]));
        assert!(launched_with(["openlight", "otro", "--minimized"]));
    }

    #[test]
    fn sin_el_argumento_abre_la_ventana() {
        assert!(!launched_with(["openlight"]));
        assert!(!launched_with(["openlight", "--otro"]));
    }

    #[test]
    fn el_nombre_del_ejecutable_no_cuenta() {
        assert!(!launched_with(["--minimized"]));
    }
}
