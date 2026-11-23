//! Ventanas de la app: la principal (que se esconde en vez de cerrarse) y la de calibración a
//! pantalla completa.

use tauri::{AppHandle, Manager, Runtime};

/// Etiqueta de la ventana principal (la de `tauri.conf.json`).
pub const MAIN: &str = "main";

/// Muestra la ventana principal y le da el foco (desde el tray, o cuando se abre una segunda
/// instancia). Si estaba escondida o minimizada, la recupera.
pub fn show_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

/// Esconde la ventana principal; la app sigue en el tray.
pub fn hide_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.hide()?;
    }
    Ok(())
}
