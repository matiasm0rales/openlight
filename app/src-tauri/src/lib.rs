//! Shell de Tauri de OpenLight: ventanas, comandos IPC, tray, autostart y permisos.
//!
//! En wave 0 los comandos son stubs (ver [`ipc`]); la tarea I1 (wave 3) los conecta con
//! `openlight-core`.

use std::sync::Arc;

pub mod autostart;
pub mod bridge;
pub mod cmd;
pub mod errors;
pub mod events;
pub mod ipc;
pub mod logging;
pub mod permissions;
pub mod shell;
pub mod state;
pub mod tray;
pub mod tray_backend;
pub mod updater;
pub mod windows;

/// Arranca la app.
pub fn run() {
    use tauri::Manager;
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
        // Solo para reiniciar después de actualizar
        .plugin(tauri_plugin_process::init())
        .plugin(autostart::plugin())
        .on_window_event(windows::on_window_event)
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            specta.mount_events(app);
            let data_dir = state::data_dir(app.handle())?;
            let log = logging::init(
                &logging::log_dir(app.handle(), &data_dir)?,
                &logging::export_dir(app.handle(), &data_dir),
            );
            let sink = Arc::new(events::TauriSink(app.handle().clone()));
            let backend = state::Backend::new(state::BackendParts::production(data_dir, sink))
                .map_err(|e| format!("no se pudo iniciar el backend: {e}"))?;
            backend.set_shell(Arc::new(shell::TauriShell(app.handle().clone())));
            backend.set_logs(log.handle());
            app.manage(log);
            app.manage(backend.clone());
            tray::install(app.handle(), Arc::new(tray_backend::AppTray::new(backend)))?;
            // Arrancó solo al iniciar sesión: queda en el tray, sin abrir la ventana.
            if autostart::started_by_autostart() {
                windows::hide_main(app.handle())?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar OpenLight")
        .run(|app, event| {
            // Salir restaura los focos (Cmd+Q, "Salir" del tray, cerrar la sesión del sistema):
            // sin esto quedarían con el color del ambiente.
            let Some(backend) = app.try_state::<Arc<state::Backend>>() else {
                return;
            };
            let backend = Arc::clone(&backend);
            match event {
                tauri::RunEvent::ExitRequested { api, .. } if !backend.is_shut_down() => {
                    api.prevent_exit();
                    if backend.begin_shutdown() {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            backend.shutdown().await;
                            app.exit(0);
                        });
                    }
                }
                // Último recurso si el cierre llegó sin pasar por ExitRequested
                tauri::RunEvent::Exit if !backend.is_shut_down() => {
                    tauri::async_runtime::block_on(backend.shutdown());
                }
                _ => {}
            }
        });
}
