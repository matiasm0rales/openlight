//! Shell de Tauri de OpenLight: ventanas, comandos IPC, tray, autostart y permisos.
//!
//! En wave 0 los comandos son stubs (ver [`ipc`]); la tarea I1 (wave 3) los conecta con
//! `openlight-core`.

use std::sync::Arc;

pub mod autostart;
pub mod ipc;
pub mod permissions;
pub mod tray;
pub mod windows;

/// Arranca la app.
pub fn run() {
    let specta = ipc::specta_builder();

    #[cfg(debug_assertions)]
    ipc::export_bindings(&specta).expect("no se pudieron exportar los bindings de TypeScript");

    tauri::Builder::default()
        // Una sola instancia: abrir la app otra vez trae la ventana principal al frente. Tiene
        // que ser el primer plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = windows::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(autostart::plugin())
        .on_window_event(windows::on_window_event)
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            // TODO(I-0): reemplazar el backend falso por uno sobre el estado real de la app
            // (`tray::TrayBackend`) y llamar a `tray::refresh` cuando cambie la sesión.
            tray::install(app.handle(), Arc::new(tray::FakeTrayBackend::demo()))?;
            // Arrancó solo al iniciar sesión: queda en el tray, sin abrir la ventana.
            if autostart::started_by_autostart() {
                windows::hide_main(app.handle())?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar OpenLight");
}
