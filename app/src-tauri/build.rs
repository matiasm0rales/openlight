/// Los comandos de la app. Declararlos hace que Tauri exija un permiso para cada uno, así cada
/// ventana puede invocar solo los que le da su capability. Un comando nuevo de `ipc.rs` se suma
/// acá y en `capabilities/default.json` (`tests/permisos.rs` falla si falta).
const COMMANDS: &[&str] = &[
    "onboarding_state",
    "onboarding_save",
    "check_permissions",
    "open_system_settings",
    "repair_screen_permission",
    "screen_repair_dismiss",
    "cloud_validate",
    "cloud_list_devices",
    "discover_lan",
    "devices_list",
    "device_add",
    "device_update",
    "device_remove",
    "devices_reorder",
    "device_test",
    "device_manual",
    "device_set_in_effect",
    "renew_key",
    "displays_list",
    "display_preview",
    "zones_set",
    "settings_get",
    "settings_set",
    "settings_reset",
    "profiles_list",
    "profiles_create",
    "profiles_update",
    "profiles_delete",
    "profiles_activate",
    "session_state",
    "session_start",
    "session_stop",
    "calibration_start",
    "calibration_set",
    "calibration_save",
    "calibration_cancel",
    "update_prepare",
    "update_abort",
    "ui_ready",
    "ui_hold",
    "backup_export",
    "backup_inspect",
    "backup_import",
    "devices_without_key",
    "fetch_missing_keys",
    "import_legacy",
    "diagnostics_log",
    "diagnostics_export",
];

fn main() {
    let mut attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // tauri-build solo agrega el manifiesto de Windows al ejecutable de la app; los binarios
        // de `cargo test` quedan sin Common Controls v6 y no arrancan (STATUS_ENTRYPOINT_NOT_FOUND).
        // Se lo agregamos a todos los binarios con el linker.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/WX");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // `capture` trae un puente en Swift (ScreenCaptureKit) que necesita el runtime de Swift del
        // sistema; sin este rpath ni la app ni sus pruebas arrancan (dyld: libswift_Concurrency).
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::try_build(attributes).expect("falló tauri-build");
}
