//! Q4 (seguridad): qué puede invocar cada ventana.
//!
//! Usa la configuración y las capabilities reales (`generate_context!`) y el verificador de
//! permisos de Tauri sobre el runtime falso. No se registra ningún plugin ni ningún comando de
//! verdad: un pedido permitido llega a un manejador vacío (o a "no existe") y uno denegado vuelve
//! con "not allowed". Nada toca el sistema.
//!
//! Se prueban **todos** los comandos de la app (los que registra `ipc.rs`) y todos los de cada
//! plugin (los de los manifiestos que genera `tauri-build`), así un permiso de más o de menos
//! hace fallar la prueba.
#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

/// Lo único de los plugins que usa la ventana principal.
const MAIN_PLUGINS: &[&str] = &[
    "plugin:event|listen",
    "plugin:event|unlisten",
    "plugin:app|version",
    "plugin:resources|close",
    "plugin:opener|open_url",
    "plugin:autostart|enable",
    "plugin:autostart|disable",
    "plugin:autostart|is_enabled",
    "plugin:updater|check",
    "plugin:updater|download",
    "plugin:updater|install",
    "plugin:updater|download_and_install",
    "plugin:process|restart",
];

/// Y la de calibración: escuchar eventos y abrir la ayuda de Tuya.
const CALIBRATION_PLUGINS: &[&str] = &[
    "plugin:event|listen",
    "plugin:event|unlisten",
    "plugin:opener|open_url",
];

/// Los comandos de la app que usa la pantalla de calibración (`Calibration.svelte`,
/// `ABTest.svelte`, el arranque de `app.svelte.ts` y las acciones de un error).
const CALIBRATION_COMMANDS: &[&str] = &[
    "onboarding_state",
    "devices_list",
    "settings_get",
    "settings_set",
    "settings_reset",
    "profiles_list",
    "session_state",
    "session_start",
    "session_stop",
    "displays_list",
    "calibration_start",
    "calibration_set",
    "calibration_save",
    "calibration_cancel",
    "renew_key",
    "open_system_settings",
    "repair_screen_permission",
    "screen_repair_dismiss",
    // El error de "el foco está siguiendo la pantalla" ofrece sacarlo del efecto
    "device_set_in_effect",
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    let path = manifest_dir().join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

fn json(path: &str) -> Value {
    serde_json::from_str(&read(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Los comandos de la app. Salen del mismo registro que usa la app (`ipc::specta_builder`),
/// exportado a una carpeta temporal: un `bindings.ts` desactualizado no esconde ninguno.
fn app_commands() -> BTreeSet<String> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bindings.ts");
    openlight_lib::ipc::specta_builder()
        .export(specta_typescript::Typescript::default(), &path)
        .unwrap();
    let commands: BTreeSet<String> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.contains("__TAURI_INVOKE<"))
        .filter_map(|line| line.split_once(">(\"")?.1.split_once('"'))
        .map(|(name, _)| name.to_owned())
        .collect();
    assert!(commands.len() > 30, "{commands:?}");
    commands
}

/// Los que declara `build.rs` (los permisos propios que generó `tauri-build`).
fn declared_in_build_rs() -> BTreeSet<String> {
    json("gen/schemas/acl-manifests.json")["__app-acl__"]["permissions"]
        .as_object()
        .expect("build.rs no declara los comandos de la app")
        .values()
        .flat_map(|p| p["commands"]["allow"].as_array().unwrap().clone())
        .map(|c| c.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn todo_comando_de_ipc_esta_registrado_y_declarado() {
    // Cada `#[tauri::command]` de `ipc.rs`...
    let source = read("src/ipc.rs");
    let written: BTreeSet<String> = source
        .split("\n#[tauri::command]")
        .skip(1)
        .filter_map(|after| after.split_once("fn ")?.1.split_once('('))
        .map(|(name, _)| name.to_owned())
        .collect();
    let registered = app_commands();
    assert_eq!(
        written, registered,
        "ipc.rs: los comandos escritos (izquierda) y los de `collect_commands!` (derecha)"
    );
    // ... y cada uno con su permiso
    let declared = declared_in_build_rs();
    let missing: Vec<_> = registered.difference(&declared).collect();
    let extra: Vec<_> = declared.difference(&registered).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "build.rs, `COMMANDS`\nfaltan: {missing:?}\nsobran: {extra:?}"
    );
}

/// `calibrationSet` → `calibration_set`.
fn snake_case(name: &str) -> String {
    name.chars()
        .flat_map(|c| {
            let separator = c.is_ascii_uppercase().then_some('_');
            separator.into_iter().chain([c.to_ascii_lowercase()])
        })
        .collect()
}

#[test]
fn la_ventana_de_calibracion_tiene_todo_lo_que_sus_pantallas_llaman() {
    for file in [
        "../ui/src/screens/Calibration.svelte",
        "../ui/src/lib/components/ABTest.svelte",
    ] {
        let source = read(file);
        let called: Vec<String> = source
            .split("commands.")
            .skip(1)
            .filter_map(|after| after.split_once('('))
            .map(|(name, _)| snake_case(name))
            .collect();
        assert!(!called.is_empty(), "{file}");
        for command in called {
            assert!(
                CALIBRATION_COMMANDS.contains(&command.as_str()),
                "{file} llama a {command}: va en capabilities/calibration.json y en \
                 CALIBRATION_COMMANDS"
            );
        }
    }
}

/// Todos los comandos de todos los plugins que la app trae compilados.
fn plugin_commands() -> BTreeSet<String> {
    let manifests = json("gen/schemas/acl-manifests.json");
    let mut commands = BTreeSet::new();
    for (plugin, manifest) in manifests.as_object().unwrap() {
        if plugin == "__app-acl__" {
            continue;
        }
        let plugin = plugin.strip_prefix("core:").unwrap_or(plugin);
        for permission in manifest["permissions"].as_object().unwrap().values() {
            for command in permission["commands"]["allow"].as_array().unwrap() {
                commands.insert(format!("plugin:{plugin}|{}", command.as_str().unwrap()));
            }
        }
    }
    for used in MAIN_PLUGINS {
        assert!(commands.contains(*used), "{used} no existe: {commands:?}");
    }
    commands
}

/// Si la ventana puede invocar `command`. `None`: se ejecutó y se rompió, así que podía.
fn allowed(window: &WebviewWindow<MockRuntime>, command: &str) -> Option<bool> {
    let url = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };
    let request = InvokeRequest {
        cmd: command.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: url.parse().unwrap(),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    // Solo un comando que llegó a ejecutarse puede romperse (los menús nativos exigen el hilo
    // principal, y las pruebas no corren ahí)
    let response = catch_unwind(AssertUnwindSafe(|| get_ipc_response(window, request))).ok()?;
    Some(match response {
        Ok(_) => true,
        Err(error) => !error.to_string().contains("not allowed"),
    })
}

/// La app con la configuración real y una ventana `label`.
fn window(label: &str) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let builder = mock_builder().invoke_handler(|invoke| {
        invoke.resolver.resolve(());
        true
    });
    #[cfg(target_os = "macos")]
    let builder = builder.enable_macos_default_menu(false);
    let app = builder
        .build(tauri::generate_context!(test = true))
        .unwrap();
    // `main` ya la crea `tauri.conf.json`
    let window = app.get_webview_window(label).unwrap_or_else(|| {
        WebviewWindowBuilder::new(&app, label, Default::default())
            .build()
            .unwrap()
    });
    (app, window)
}

#[test]
fn cada_ventana_puede_exactamente_lo_que_usa() {
    let app_commands = app_commands();
    let plugin_commands = plugin_commands();
    let all: Vec<&String> = app_commands.iter().chain(&plugin_commands).collect();

    for (label, plugins, commands) in [
        ("main", MAIN_PLUGINS, None),
        (
            "calibration",
            CALIBRATION_PLUGINS,
            Some(CALIBRATION_COMMANDS),
        ),
        // Una ventana que nadie declaró no puede nada
        ("otra", &[], Some(&[][..])),
    ] {
        let expected: BTreeSet<&str> = match commands {
            Some(commands) => commands.to_vec(),
            None => app_commands.iter().map(String::as_str).collect(),
        }
        .into_iter()
        .chain(plugins.iter().copied())
        .collect();
        let mut current = window(label);
        let mut got: BTreeSet<&str> = BTreeSet::new();
        for command in &all {
            let result = allowed(&current.1, command);
            if result.is_none() {
                // Quedó a medio ejecutar: se sigue con una app nueva
                current = window(label);
            }
            if result.unwrap_or(true) {
                got.insert(command);
            }
        }
        let extra: Vec<_> = got.difference(&expected).collect();
        let missing: Vec<_> = expected.difference(&got).collect();
        assert!(
            extra.is_empty() && missing.is_empty(),
            "ventana «{label}»\nde más: {extra:?}\nfaltan: {missing:?}\n\
             (un comando nuevo se declara en build.rs y se permite en capabilities/)"
        );
    }
}

/// Los `url` permitidos de `opener:allow-open-url` en una capability.
fn url_scope(capability: &str) -> Vec<glob::Pattern> {
    json(capability)["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["identifier"] == "opener:allow-open-url")
        .flat_map(|p| p["allow"].as_array().unwrap().clone())
        .map(|entry| glob::Pattern::new(entry["url"].as_str().unwrap()).unwrap())
        .collect()
}

/// Los valores `"url"` de la guía (en cualquier nivel).
fn guide_urls(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, field) in map {
                match field.as_str() {
                    Some(url) if key == "url" => out.push(url.to_owned()),
                    _ => guide_urls(field, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| guide_urls(v, out)),
        _ => {}
    }
}

#[test]
fn solo_se_pueden_abrir_las_paginas_que_la_app_enlaza() {
    let mut guide = Vec::new();
    for language in ["es", "en"] {
        guide_urls(
            &json(&format!("../ui/src/content/guide/{language}.json")),
            &mut guide,
        );
    }
    assert!(!guide.is_empty());
    guide.push(tuya_cloud::error::TUYA_IOT_URL.to_owned());
    let releases = read("../ui/src/lib/updates.ts")
        .split_once("RELEASES_URL = \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(url, _)| url.to_owned())
        .expect("RELEASES_URL en updates.ts");

    for (capability, urls) in [
        (
            "capabilities/default.json",
            guide.iter().chain([&releases]).collect::<Vec<_>>(),
        ),
        ("capabilities/calibration.json", guide.iter().collect()),
    ] {
        let scope = url_scope(capability);
        let permitted = |url: &str| scope.iter().any(|p| p.matches(url));
        for url in urls {
            assert!(permitted(url), "{capability}: falta permitir {url}");
        }
        for url in [
            "https://example.com/",
            "https://iot.tuya.com.example.com/",
            "http://iot.tuya.com/",
            "file:///etc/passwd",
            "mailto:alguien@example.com",
            "https://github.com/otro/repo",
        ] {
            assert!(!permitted(url), "{capability}: permite {url}");
        }
    }
}
