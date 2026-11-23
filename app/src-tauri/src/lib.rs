//! Shell de Tauri de OpenLight: ventanas, comandos IPC, tray, autostart y permisos.
//!
//! En wave 0 los comandos son stubs (ver [`ipc`]); la tarea I1 (wave 3) los conecta con
//! `openlight-core`.

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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar OpenLight");
}
