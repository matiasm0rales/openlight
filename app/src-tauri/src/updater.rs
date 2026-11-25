//! Lo que la app necesita alrededor de `tauri-plugin-updater`.
//!
//! La búsqueda, la descarga y la instalación las pide la UI al plugin (solo la ventana
//! principal tiene el permiso). Antes de instalar llama a `update_prepare`, que devuelve los
//! focos como al salir ([`crate::state::Backend::update_prepare`]).

use std::path::Path;

/// Archivo que marca que el próximo arranque viene de una actualización.
const REOPEN_MARKER: &str = "abrir-despues-de-actualizar";

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
