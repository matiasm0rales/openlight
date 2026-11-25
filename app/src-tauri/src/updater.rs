//! Lo que la app necesita alrededor de `tauri-plugin-updater`.
//!
//! La búsqueda, la descarga y la instalación las pide la UI al plugin (solo la ventana
//! principal tiene el permiso). Antes de instalar llama a `update_prepare`, que devuelve los
//! focos como al salir ([`crate::state::Backend::update_prepare`]).

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

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
