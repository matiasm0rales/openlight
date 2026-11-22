//! Cada ajuste, opción y código de error tiene su texto en español y en inglés.

use std::path::PathBuf;

use openlight_types::{ErrorCode, SettingKind, catalog};

fn locale(lang: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/ui/src/lib/i18n")
        .join(format!("{lang}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn lookup<'a>(root: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    key.split('.')
        .try_fold(root, |v, part| v.get(part))?
        .as_str()
}

#[test]
fn cada_clave_tiene_texto_en_los_dos_idiomas() {
    for lang in ["es", "en"] {
        let root = locale(lang);
        let mut keys = Vec::new();
        for def in catalog() {
            keys.push(def.title_key.clone());
            keys.push(def.help_key.clone());
            if let SettingKind::Choice { options } = &def.kind {
                keys.extend(
                    options
                        .iter()
                        .map(|o| format!("settings.{}.option.{o}", def.id)),
                );
            }
        }
        for code in ErrorCode::ALL {
            keys.push(code.title_key());
            keys.push(code.detail_key());
        }
        let faltan: Vec<_> = keys
            .iter()
            .filter(|k| lookup(&root, k).is_none_or(str::is_empty))
            .collect();
        assert!(faltan.is_empty(), "faltan en {lang}.json: {faltan:?}");
    }
}
