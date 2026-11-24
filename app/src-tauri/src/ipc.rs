//! Contrato IPC: la lista **cerrada** de comandos y eventos entre la UI y el backend.
//!
//! - Los DTOs viven en [`openlight_types::ipc`]; acá solo están los comandos.
//! - Los tipos de TypeScript se generan en `app/ui/src/lib/bindings.ts` con `tauri-specta`
//!   (al arrancar en debug y con `cargo test -p openlight`). El CI falla si quedan desactualizados.
//! - Agregar, quitar o cambiar un comando o evento pasa por el integrador (I-0).
//!
//! En wave 0 casi todos los comandos devuelven [`ErrorCode::NotImplemented`]; `onboarding_state`,
//! `settings_get` y los de solo lectura devuelven datos por defecto para que la UI arranque.

use openlight_types::ipc::{
    CalibrationState, CloudCredentialsInput, CloudDeviceInfo, ConfiguredDevice, DeviceAddRequest,
    DeviceHealthEvent, DiscoveredDevice, ImportReport, LiveColorEvent, LogLine, ManualSet,
    OnboardingState, OnboardingStep, PermissionStatus, PipelineStatsEvent, PreviewImage,
    SessionStateEvent, SettingScope, SettingsSnapshot, TestPattern, events,
};
use openlight_types::{
    CloudAccount, DeviceId, DisplayId, DisplayInfo, ErrorCode, Profile, ProfileId, SessionState,
    SettingId, SettingValue, SystemPane, UserFacingError, Zone,
};
use std::sync::Arc;

use tauri::State;
use tauri_specta::{Builder, collect_commands};

use crate::state::Backend;

pub use crate::errors::CmdResult;

/// El estado de la app, tal como lo reciben los comandos.
pub type App<'a> = State<'a, Arc<Backend>>;

fn todo<T>(command: &str) -> CmdResult<T> {
    Err(UserFacingError::not_implemented(command))
}

// ---------- Asistente y permisos ----------

/// Progreso del asistente inicial.
#[tauri::command]
#[specta::specta]
pub async fn onboarding_state(app: App<'_>) -> CmdResult<OnboardingState> {
    app.onboarding_state()
}

/// Guarda el progreso del asistente (para pausar y retomar).
#[tauri::command]
#[specta::specta]
pub async fn onboarding_save(
    app: App<'_>,
    step: OnboardingStep,
    completed: Vec<OnboardingStep>,
) -> CmdResult<OnboardingState> {
    app.onboarding_save(step, completed)
}

/// Estado de los permisos del sistema que necesita la app en este SO.
#[tauri::command]
#[specta::specta]
pub async fn check_permissions() -> CmdResult<Vec<PermissionStatus>> {
    todo("check_permissions")
}

/// Abre el panel exacto de los ajustes del sistema.
#[tauri::command]
#[specta::specta]
pub async fn open_system_settings(pane: SystemPane) -> CmdResult<()> {
    let _ = pane;
    todo("open_system_settings")
}

// ---------- Nube y red local ----------

/// Valida Access ID / Access Secret y guarda las credenciales en el llavero.
#[tauri::command]
#[specta::specta]
pub async fn cloud_validate(
    app: App<'_>,
    credentials: CloudCredentialsInput,
) -> CmdResult<CloudAccount> {
    app.cloud_validate(credentials).await
}

/// Lista los dispositivos de la cuenta, cruzados con lo encontrado en la red local.
#[tauri::command]
#[specta::specta]
pub async fn cloud_list_devices(app: App<'_>) -> CmdResult<Vec<CloudDeviceInfo>> {
    app.cloud_list_devices().await
}

/// Busca focos en la red local durante `timeout_s` segundos (también emite `discovery://found`).
#[tauri::command]
#[specta::specta]
pub async fn discover_lan(app: App<'_>, timeout_s: f32) -> CmdResult<Vec<DiscoveredDevice>> {
    app.discover_lan(timeout_s).await
}

// ---------- Dispositivos ----------

/// Focos configurados.
#[tauri::command]
#[specta::specta]
pub async fn devices_list(app: App<'_>) -> CmdResult<Vec<ConfiguredDevice>> {
    app.devices_list()
}

/// Agrega un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_add(app: App<'_>, request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
    app.device_add(request).await
}

/// Cambia el nombre o la IP de un foco (`ip = None` = buscarla automáticamente).
#[tauri::command]
#[specta::specta]
pub async fn device_update(app: App<'_>, request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
    app.device_update(request)
}

/// Quita un foco (y su clave del llavero).
#[tauri::command]
#[specta::specta]
pub async fn device_remove(app: App<'_>, device: DeviceId) -> CmdResult<()> {
    app.device_remove(device).await
}

/// Corre un patrón de prueba en un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_test(app: App<'_>, device: DeviceId, pattern: TestPattern) -> CmdResult<()> {
    app.device_test(device, pattern).await
}

/// Control manual de un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_manual(app: App<'_>, device: DeviceId, set: ManualSet) -> CmdResult<()> {
    app.device_manual(device, set).await
}

/// Renueva la clave local con las credenciales guardadas (un clic; también lo ofrece el 914).
#[tauri::command]
#[specta::specta]
pub async fn renew_key(app: App<'_>, device: DeviceId) -> CmdResult<()> {
    app.renew_key(device).await
}

// ---------- Pantallas y zonas ----------

/// Monitores disponibles.
#[tauri::command]
#[specta::specta]
pub async fn displays_list(app: App<'_>) -> CmdResult<Vec<DisplayInfo>> {
    app.displays_list()
}

/// Vista previa de un monitor.
#[tauri::command]
#[specta::specta]
pub async fn display_preview(app: App<'_>, display: DisplayId) -> CmdResult<PreviewImage> {
    app.display_preview(display).await
}

/// Define las zonas que sigue un foco.
#[tauri::command]
#[specta::specta]
pub async fn zones_set(app: App<'_>, device: DeviceId, zones: Vec<Zone>) -> CmdResult<()> {
    app.zones_set(device, zones)
}

// ---------- Ajustes y perfiles ----------

/// Catálogo y valores de los ajustes.
#[tauri::command]
#[specta::specta]
pub async fn settings_get(app: App<'_>) -> CmdResult<SettingsSnapshot> {
    app.settings_get()
}

/// Cambia un ajuste (validado contra el catálogo). Vale desde la próxima sesión.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    app: App<'_>,
    scope: SettingScope,
    id: SettingId,
    value: SettingValue,
) -> CmdResult<SettingsSnapshot> {
    app.settings_set(scope, id, value)
}

/// Vuelve un ajuste (o todos, con `id = None`) a su valor por defecto.
#[tauri::command]
#[specta::specta]
pub async fn settings_reset(
    app: App<'_>,
    scope: SettingScope,
    id: Option<SettingId>,
) -> CmdResult<SettingsSnapshot> {
    app.settings_reset(scope, id)
}

/// Perfiles.
#[tauri::command]
#[specta::specta]
pub async fn profiles_list(app: App<'_>) -> CmdResult<Vec<Profile>> {
    app.profiles_list()
}

/// Crea un perfil, vacío o copiando otro.
#[tauri::command]
#[specta::specta]
pub async fn profiles_create(
    app: App<'_>,
    name: String,
    from: Option<ProfileId>,
) -> CmdResult<Profile> {
    app.profiles_create(name, from)
}

/// Guarda un perfil (nombre, valores, atajo).
#[tauri::command]
#[specta::specta]
pub async fn profiles_update(app: App<'_>, profile: Profile) -> CmdResult<Profile> {
    app.profiles_update(profile)
}

/// Borra un perfil propio.
#[tauri::command]
#[specta::specta]
pub async fn profiles_delete(app: App<'_>, id: ProfileId) -> CmdResult<()> {
    app.profiles_delete(id)
}

/// Activa un perfil (`None` = solo los ajustes globales).
#[tauri::command]
#[specta::specta]
pub async fn profiles_activate(app: App<'_>, id: Option<ProfileId>) -> CmdResult<()> {
    app.profiles_activate(id)
}

// ---------- Sesión ----------

/// Estado actual de la sesión (después llegan los cambios por `session://state`).
#[tauri::command]
#[specta::specta]
pub async fn session_state(app: App<'_>) -> CmdResult<SessionState> {
    Ok(app.session_state())
}

/// Arranca el efecto.
#[tauri::command]
#[specta::specta]
pub async fn session_start(app: App<'_>) -> CmdResult<SessionState> {
    app.session_start().await
}

/// Detiene el efecto y devuelve los focos a su estado.
#[tauri::command]
#[specta::specta]
pub async fn session_stop(app: App<'_>) -> CmdResult<SessionState> {
    app.session_stop().await
}

// ---------- Calibración ----------

/// Abre la calibración a pantalla completa en `display` (`quick` = 6 tonos en vez de 12).
#[tauri::command]
#[specta::specta]
pub async fn calibration_start(
    app: App<'_>,
    device: DeviceId,
    display: DisplayId,
    quick: bool,
) -> CmdResult<CalibrationState> {
    app.calibration_start(device, display, quick).await
}

/// Ajusta el tono `index` y mueve el foco en vivo (`None` = dejar ese valor como está).
#[tauri::command]
#[specta::specta]
pub async fn calibration_set(
    app: App<'_>,
    index: u32,
    tono_deg: Option<f32>,
    sat: Option<f32>,
) -> CmdResult<CalibrationState> {
    app.calibration_set(index, tono_deg, sat).await
}

/// Guarda la tabla del foco y cierra la calibración.
#[tauri::command]
#[specta::specta]
pub async fn calibration_save(app: App<'_>) -> CmdResult<()> {
    app.calibration_save().await
}

/// Cierra la calibración sin guardar.
#[tauri::command]
#[specta::specta]
pub async fn calibration_cancel(app: App<'_>) -> CmdResult<()> {
    app.calibration_cancel().await
}

// ---------- Importar y diagnóstico ----------

/// Importa el setup del script Python desde una carpeta.
#[tauri::command]
#[specta::specta]
pub async fn import_legacy(folder: String) -> CmdResult<ImportReport> {
    let _ = folder;
    todo("import_legacy")
}

/// Últimas líneas del log (las más nuevas al final), para el visor del diagnóstico.
#[tauri::command]
#[specta::specta]
pub async fn diagnostics_log(limit: Option<u32>) -> CmdResult<Vec<LogLine>> {
    let _ = limit;
    Ok(Vec::new())
}

/// Exporta un zip de diagnóstico sin secretos y devuelve su ruta.
#[tauri::command]
#[specta::specta]
pub async fn diagnostics_export(destination: Option<String>) -> CmdResult<String> {
    let _ = destination;
    todo("diagnostics_export")
}

// ---------- Registro ----------

/// Arma el builder de `tauri-specta` con todos los comandos, los tipos de eventos y sus nombres.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            onboarding_state,
            onboarding_save,
            check_permissions,
            open_system_settings,
            cloud_validate,
            cloud_list_devices,
            discover_lan,
            devices_list,
            device_add,
            device_update,
            device_remove,
            device_test,
            device_manual,
            renew_key,
            displays_list,
            display_preview,
            zones_set,
            settings_get,
            settings_set,
            settings_reset,
            profiles_list,
            profiles_create,
            profiles_update,
            profiles_delete,
            profiles_activate,
            session_state,
            session_start,
            session_stop,
            calibration_start,
            calibration_set,
            calibration_save,
            calibration_cancel,
            import_legacy,
            diagnostics_log,
            diagnostics_export,
        ])
        .typ::<SessionStateEvent>()
        .typ::<DeviceHealthEvent>()
        .typ::<LiveColorEvent>()
        .typ::<UserFacingError>()
        .typ::<DiscoveredDevice>()
        .typ::<ErrorCode>()
        .typ::<PipelineStatsEvent>()
        .constant("EVENT_SESSION_STATE", events::SESSION_STATE)
        .constant("EVENT_DEVICE_HEALTH", events::DEVICE_HEALTH)
        .constant("EVENT_COLOR_LIVE", events::COLOR_LIVE)
        .constant("EVENT_ERROR_RAISED", events::ERROR_RAISED)
        .constant("EVENT_DISCOVERY_FOUND", events::DISCOVERY_FOUND)
        .constant("EVENT_PIPELINE_STATS", events::PIPELINE_STATS)
}

/// Ruta de los bindings generados.
pub const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/src/lib/bindings.ts");

/// Escribe `app/ui/src/lib/bindings.ts`.
pub fn export_bindings(builder: &Builder<tauri::Wry>) -> Result<(), String> {
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// Generado por tauri-specta desde app/src-tauri/src/ipc.rs. No editar a mano.\n// @ts-nocheck"),
            BINDINGS_PATH,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenera los bindings; el CI verifica después que no haya diferencias con lo commiteado.
    #[test]
    fn exporta_los_bindings() {
        export_bindings(&specta_builder()).unwrap();
        let ts = std::fs::read_to_string(BINDINGS_PATH).unwrap();
        for cmd in [
            "onboardingState",
            "settingsGet",
            "sessionStart",
            "importLegacy",
        ] {
            assert!(ts.contains(cmd), "falta {cmd} en bindings.ts");
        }
        assert!(ts.contains("session://state"));
    }
}
