//! Shell de Tauri de OpenLight: ventanas, comandos IPC, tray, autostart y permisos.
//!
//! En wave 0 los comandos son stubs (ver [`ipc`]); la tarea I1 (wave 3) los conecta con
//! `openlight-core`.

pub mod autostart;
pub mod ipc;
pub mod permissions;

/// Arranca la app.
pub fn run() {
    let specta = ipc::specta_builder();

    #[cfg(debug_assertions)]
    ipc::export_bindings(&specta).expect("no se pudieron exportar los bindings de TypeScript");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(autostart::plugin())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar OpenLight");
}
