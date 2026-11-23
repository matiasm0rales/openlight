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
    SettingId, SettingValue, Settings, SystemPane, UserFacingError, Zone, catalog,
};
use tauri_specta::{Builder, collect_commands};

/// Resultado de todos los comandos: los errores siempre llegan listos para mostrar.
pub type CmdResult<T> = Result<T, UserFacingError>;

fn todo<T>(command: &str) -> CmdResult<T> {
    Err(UserFacingError::not_implemented(command))
}

// ---------- Asistente y permisos ----------

/// Progreso del asistente inicial.
#[tauri::command]
#[specta::specta]
pub async fn onboarding_state() -> CmdResult<OnboardingState> {
    Ok(OnboardingState {
        step: OnboardingStep::Welcome,
        completed: Vec::new(),
        has_credentials: false,
        devices: Vec::new(),
    })
}

/// Guarda el progreso del asistente (para pausar y retomar).
#[tauri::command]
#[specta::specta]
pub async fn onboarding_save(
    step: OnboardingStep,
    completed: Vec<OnboardingStep>,
) -> CmdResult<OnboardingState> {
    let _ = (step, completed);
    todo("onboarding_save")
}

/// Estado de los permisos del sistema que necesita la app en este SO.
#[tauri::command]
#[specta::specta]
pub async fn check_permissions() -> CmdResult<Vec<PermissionStatus>> {
    Ok(crate::permissions::check_blocking().await)
}

/// Abre el panel exacto de los ajustes del sistema.
#[tauri::command]
#[specta::specta]
pub async fn open_system_settings(pane: SystemPane) -> CmdResult<()> {
    crate::permissions::open_settings_blocking(pane).await
}

// ---------- Nube y red local ----------

/// Valida Access ID / Access Secret y guarda las credenciales en el llavero.
#[tauri::command]
#[specta::specta]
pub async fn cloud_validate(credentials: CloudCredentialsInput) -> CmdResult<CloudAccount> {
    let _ = credentials;
    todo("cloud_validate")
}

/// Lista los dispositivos de la cuenta, cruzados con lo encontrado en la red local.
#[tauri::command]
#[specta::specta]
pub async fn cloud_list_devices() -> CmdResult<Vec<CloudDeviceInfo>> {
    todo("cloud_list_devices")
}

/// Busca focos en la red local durante `timeout_s` segundos (también emite `discovery://found`).
#[tauri::command]
#[specta::specta]
pub async fn discover_lan(timeout_s: f32) -> CmdResult<Vec<DiscoveredDevice>> {
    let _ = timeout_s;
    todo("discover_lan")
}

// ---------- Dispositivos ----------

/// Focos configurados.
#[tauri::command]
#[specta::specta]
pub async fn devices_list() -> CmdResult<Vec<ConfiguredDevice>> {
    Ok(Vec::new())
}

/// Agrega un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_add(request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
    let _ = request;
    todo("device_add")
}

/// Cambia el nombre o la IP de un foco (`ip = None` = buscarla automáticamente).
#[tauri::command]
#[specta::specta]
pub async fn device_update(request: DeviceAddRequest) -> CmdResult<ConfiguredDevice> {
    let _ = request;
    todo("device_update")
}

/// Quita un foco (y su clave del llavero).
#[tauri::command]
#[specta::specta]
pub async fn device_remove(device: DeviceId) -> CmdResult<()> {
    let _ = device;
    todo("device_remove")
}

/// Corre un patrón de prueba en un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_test(device: DeviceId, pattern: TestPattern) -> CmdResult<()> {
    let _ = (device, pattern);
    todo("device_test")
}

/// Control manual de un foco.
#[tauri::command]
#[specta::specta]
pub async fn device_manual(device: DeviceId, set: ManualSet) -> CmdResult<()> {
    let _ = (device, set);
    todo("device_manual")
}

/// Renueva la clave local con las credenciales guardadas (un clic; también lo ofrece el 914).
#[tauri::command]
#[specta::specta]
pub async fn renew_key(device: DeviceId) -> CmdResult<()> {
    let _ = device;
    todo("renew_key")
}

// ---------- Pantallas y zonas ----------

/// Monitores disponibles.
#[tauri::command]
#[specta::specta]
pub async fn displays_list() -> CmdResult<Vec<DisplayInfo>> {
    Ok(Vec::new())
}

/// Vista previa de un monitor.
#[tauri::command]
#[specta::specta]
pub async fn display_preview(display: DisplayId) -> CmdResult<PreviewImage> {
    let _ = display;
    todo("display_preview")
}

/// Define las zonas que sigue un foco.
#[tauri::command]
#[specta::specta]
pub async fn zones_set(device: DeviceId, zones: Vec<Zone>) -> CmdResult<()> {
    let _ = (device, zones);
    todo("zones_set")
}

// ---------- Ajustes y perfiles ----------

fn default_snapshot() -> SettingsSnapshot {
    SettingsSnapshot {
        catalog: catalog().to_vec(),
        effective: Settings::default(),
        global: Settings::default(),
        devices: Vec::new(),
        active_profile: None,
    }
}

/// Catálogo y valores de los ajustes.
#[tauri::command]
#[specta::specta]
pub async fn settings_get() -> CmdResult<SettingsSnapshot> {
    Ok(default_snapshot())
}

/// Cambia un ajuste (validado contra el catálogo).
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    scope: SettingScope,
    id: SettingId,
    value: SettingValue,
) -> CmdResult<SettingsSnapshot> {
    id.def().validate(&value)?;
    let _ = scope;
    todo("settings_set")
}

/// Vuelve un ajuste (o todos, con `id = None`) a su valor por defecto.
#[tauri::command]
#[specta::specta]
pub async fn settings_reset(
    scope: SettingScope,
    id: Option<SettingId>,
) -> CmdResult<SettingsSnapshot> {
    let _ = (scope, id);
    todo("settings_reset")
}

/// Perfiles.
#[tauri::command]
#[specta::specta]
pub async fn profiles_list() -> CmdResult<Vec<Profile>> {
    Ok(Vec::new())
}

/// Crea un perfil, vacío o copiando otro.
#[tauri::command]
#[specta::specta]
pub async fn profiles_create(name: String, from: Option<ProfileId>) -> CmdResult<Profile> {
    let _ = (name, from);
    todo("profiles_create")
}

/// Guarda un perfil (nombre, valores, atajo).
#[tauri::command]
#[specta::specta]
pub async fn profiles_update(profile: Profile) -> CmdResult<Profile> {
    let _ = profile;
    todo("profiles_update")
}

/// Borra un perfil propio.
#[tauri::command]
#[specta::specta]
pub async fn profiles_delete(id: ProfileId) -> CmdResult<()> {
    let _ = id;
    todo("profiles_delete")
}

/// Activa un perfil (`None` = solo los ajustes globales).
#[tauri::command]
#[specta::specta]
pub async fn profiles_activate(id: Option<ProfileId>) -> CmdResult<()> {
    let _ = id;
    todo("profiles_activate")
}

// ---------- Sesión ----------

/// Estado actual de la sesión (después llegan los cambios por `session://state`).
#[tauri::command]
#[specta::specta]
pub async fn session_state() -> CmdResult<SessionState> {
    Ok(SessionState::Idle)
}

/// Arranca el efecto.
#[tauri::command]
#[specta::specta]
pub async fn session_start() -> CmdResult<SessionState> {
    todo("session_start")
}

/// Detiene el efecto y devuelve los focos a su estado.
#[tauri::command]
#[specta::specta]
pub async fn session_stop() -> CmdResult<SessionState> {
    Ok(SessionState::Idle)
}

// ---------- Calibración ----------

/// Abre la calibración a pantalla completa en `display` (`quick` = 6 tonos en vez de 12).
#[tauri::command]
#[specta::specta]
pub async fn calibration_start(
    device: DeviceId,
    display: DisplayId,
    quick: bool,
) -> CmdResult<CalibrationState> {
    let _ = (device, display, quick);
    todo("calibration_start")
}

/// Ajusta el tono `index` y mueve el foco en vivo.
#[tauri::command]
#[specta::specta]
pub async fn calibration_set(index: u32, tono_deg: f32, sat: f32) -> CmdResult<CalibrationState> {
    let _ = (index, tono_deg, sat);
    todo("calibration_set")
}

/// Guarda la tabla del foco y cierra la calibración.
#[tauri::command]
#[specta::specta]
pub async fn calibration_save() -> CmdResult<()> {
    todo("calibration_save")
}

/// Cierra la calibración sin guardar.
#[tauri::command]
#[specta::specta]
pub async fn calibration_cancel() -> CmdResult<()> {
    todo("calibration_cancel")
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

    #[test]
    fn los_stubs_devuelven_errores_con_accion() {
        let e = tauri::async_runtime::block_on(session_start()).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotImplemented);
        assert!(!e.actions.is_empty());
    }
}
