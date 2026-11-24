//! Shell de Tauri de OpenLight: ventanas, comandos IPC, tray, autostart y permisos.
//!
//! En wave 0 los comandos son stubs (ver [`ipc`]); la tarea I1 (wave 3) los conecta con
//! `openlight-core`.

pub mod bridge;
pub mod cmd;
pub mod errors;
pub mod events;
pub mod ipc;
pub mod state;

/// Arranca la app.
pub fn run() {
    use tauri::Manager;
    let specta = ipc::specta_builder();

    #[cfg(debug_assertions)]
    ipc::export_bindings(&specta).expect("no se pudieron exportar los bindings de TypeScript");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            let data_dir = state::data_dir(app.handle())?;
            let sink = std::sync::Arc::new(events::TauriSink(app.handle().clone()));
            let backend = state::Backend::new(state::BackendParts::production(data_dir, sink))
                .map_err(|e| format!("no se pudo iniciar el backend: {e}"))?;
            app.manage(backend);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar OpenLight");
}
