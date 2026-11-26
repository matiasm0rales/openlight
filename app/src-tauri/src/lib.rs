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

/// El contexto de Tauri (configuración, íconos y archivos de la UI). Es una función aparte para
/// que `examples/banco.rs` use el mismo: generarlo dos veces embebe `Info.plist` dos veces.
#[doc(hidden)]
pub fn context() -> tauri::Context {
    tauri::generate_context!()
}

/// Arranca la app.
pub fn run() {
    use tauri::Manager;
    let specta = ipc::specta_builder();

    #[cfg(debug_assertions)]
    ipc::export_bindings(&specta).expect("no se pudieron exportar los bindings de TypeScript");

    let mut context = context();
    // Solo en desarrollo: probar el updater contra un `latest.json` local (docs/release.md).
    let dev_endpoint = updater::apply_dev_endpoint(&mut context.config_mut().plugins);

    tauri::Builder::default()
        // Una sola instancia: abrir la app otra vez trae la ventana principal al frente. Tiene
        // que ser el primer plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
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
            match dev_endpoint {
                Some(Ok(endpoint)) => {
                    tracing::warn!(%endpoint, "actualizaciones desde un endpoint de desarrollo");
                }
                Some(Err(error)) => {
                    tracing::warn!(%error, "{} se ignora", updater::ENDPOINT_ENV);
                }
                None => {}
            }
            let sink = Arc::new(events::TauriSink(app.handle().clone()));
            let backend = state::Backend::new(state::BackendParts::production(data_dir, sink))
                .map_err(|e| format!("no se pudo iniciar el backend: {e}"))?;
            backend.set_shell(Arc::new(shell::TauriShell(app.handle().clone())));
            backend.set_logs(log.handle());
            app.manage(log);
            app.manage(backend.clone());
            let updated = backend.take_reopen_after_update();
            tray::install(app.handle(), Arc::new(tray_backend::AppTray::new(backend)))?;
            // Arrancó sola al iniciar sesión: queda en el tray, sin crear la ventana (salvo que
            // acabe de actualizarse desde ella).
            if updated || !autostart::started_by_autostart() {
                windows::open_main(app.handle())?;
            } else {
                windows::sync_visibility(app.handle(), None);
            }
            Ok(())
        })
        .build(context)
        .expect("error al iniciar OpenLight")
        .run(|app, event| {
            windows::keep_running(&event);
            // En macOS abrir la app de nuevo (Finder, Dock) no lanza otra instancia: avisa acá.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                windows::show_main(app);
            }
            // Salir restaura los focos (Cmd+Q, "Salir" del tray, cerrar la sesión del sistema):
            // sin esto quedarían con el color del ambiente. Un reinicio no se puede frenar
            // (Tauri ignora `prevent_exit`), así que sus focos se restauran en `Exit`; después
            // de una actualización ya llegan restaurados (`update_prepare`).
            let Some(backend) = app.try_state::<Arc<state::Backend>>() else {
                return;
            };
            let backend = Arc::clone(&backend);
            match event {
                // Sin código es que se cerró la última ventana (ver `keep_running`)
                tauri::RunEvent::ExitRequested {
                    api,
                    code: Some(code),
                    ..
                } if code != tauri::RESTART_EXIT_CODE && !backend.is_shut_down() => {
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
