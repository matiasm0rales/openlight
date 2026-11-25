//! Lo que la app necesita alrededor de `tauri-plugin-updater`.
//!
//! La búsqueda, la descarga y la instalación las pide la UI al plugin (solo la ventana
//! principal tiene el permiso). Antes de instalar llama a `update_prepare`, que devuelve los
//! focos como al salir ([`crate::state::Backend::update_prepare`]).

use std::path::Path;

use tauri::utils::config::PluginConfig;

/// Variable de entorno que cambia dónde se busca `latest.json`. Solo en builds de desarrollo.
pub const ENDPOINT_ENV: &str = "OPENLIGHT_UPDATER_ENDPOINT";

/// Archivo que marca que el próximo arranque viene de una actualización.
const REOPEN_MARKER: &str = "abrir-despues-de-actualizar";

/// Cambia el endpoint del updater en la configuración (antes de arrancar los plugins).
pub fn override_endpoint(plugins: &mut PluginConfig, endpoint: &str) -> Result<(), String> {
    let url: tauri::Url = endpoint
        .parse()
        .map_err(|e| format!("{endpoint:?} no es una URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{endpoint:?} tiene que ser http o https"));
    }
    let updater = plugins
        .0
        .get_mut("updater")
        .and_then(|v| v.as_object_mut())
        .ok_or("falta plugins.updater en tauri.conf.json")?;
    updater.insert("endpoints".into(), serde_json::json!([url.as_str()]));
    Ok(())
}

/// Aplica [`ENDPOINT_ENV`] si está puesta. En un build de release no hace nada: de dónde
/// salen las actualizaciones no puede depender del entorno. `Ok` trae el endpoint en uso.
pub fn apply_dev_endpoint(plugins: &mut PluginConfig) -> Option<Result<String, String>> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let endpoint = std::env::var(ENDPOINT_ENV).ok().filter(|v| !v.is_empty())?;
    Some(override_endpoint(plugins, &endpoint).map(|()| endpoint))
}

/// Antes de actualizar: el próximo arranque tiene que mostrar la ventana, aunque la app se
/// haya abierto sola al iniciar sesión (el reinicio repite los argumentos del arranque).
pub fn mark_reopen(data_dir: &Path) {
    if let Err(error) = std::fs::create_dir_all(data_dir)
        .and_then(|()| std::fs::write(data_dir.join(REOPEN_MARKER), b""))
    {
        tracing::warn!(%error, "no se pudo anotar que hay que reabrir la ventana");
    }
}

/// Si el arranque viene de una actualización; la marca se borra al leerla.
pub fn take_reopen(data_dir: &Path) -> bool {
    std::fs::remove_file(data_dir.join(REOPEN_MARKER)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tauri::utils::config::PluginConfig;

    fn plugins() -> PluginConfig {
        serde_json::from_value(json!({
            "updater": { "pubkey": "clave", "endpoints": ["https://ejemplo.com/latest.json"] }
        }))
        .unwrap()
    }

    #[test]
    fn el_endpoint_de_desarrollo_reemplaza_al_configurado_y_respeta_la_clave() {
        let mut plugins = plugins();
        override_endpoint(&mut plugins, "http://localhost:8123/latest.json").unwrap();
        assert_eq!(
            plugins.0["updater"],
            json!({ "pubkey": "clave", "endpoints": ["http://localhost:8123/latest.json"] })
        );
    }

    #[test]
    fn un_endpoint_que_no_es_una_url_web_se_rechaza_sin_tocar_la_configuracion() {
        for malo in ["", "latest.json", "file:///tmp/latest.json", "http//x"] {
            let mut plugins = plugins();
            assert!(override_endpoint(&mut plugins, malo).is_err(), "{malo:?}");
            assert_eq!(plugins.0, self::plugins().0);
        }
    }

    #[test]
    fn sin_updater_configurado_no_hay_nada_que_reemplazar() {
        let mut vacio = PluginConfig::default();
        assert!(override_endpoint(&mut vacio, "http://localhost/latest.json").is_err());
    }

    #[test]
    fn la_marca_para_reabrir_la_ventana_se_consume_una_sola_vez() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!take_reopen(dir.path()));
        mark_reopen(dir.path());
        assert!(take_reopen(dir.path()));
        assert!(!take_reopen(dir.path()));
    }

    #[test]
    fn solo_la_ventana_principal_puede_actualizar_y_reiniciar() {
        let permisos = |json: &str| -> Value { serde_json::from_str(json).unwrap() };
        let principal = permisos(include_str!("../capabilities/default.json"));
        assert_eq!(principal["windows"], json!(["main"]));
        let de_principal = principal["permissions"].as_array().unwrap();
        for permiso in ["updater:default", "process:allow-restart"] {
            assert!(de_principal.contains(&json!(permiso)), "falta {permiso}");
        }
        assert!(
            !de_principal.contains(&json!("process:default")),
            "no hace falta salir"
        );
        let calibracion = include_str!("../capabilities/calibration.json");
        assert!(!calibracion.contains("updater:") && !calibracion.contains("process:"));
    }
}
