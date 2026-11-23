# Registro de decisiones

## 2026-11-22 — Nombre "OpenLight" (T0.1)

Revisión antes de crear el repo:

- **crates.io:** `openlight`, `openlight-types` y `openlight-core` libres.
- **GitHub:** `matiasm0rales/openlight` libre. Hay varios repos llamados *openlight*; el más
  cercano es `antoniobertob/openlight` ("Open source ambilight project", 2 estrellas).
- **Marcas:** `OPENLIGHT` está registrada en EE. UU. por OpenLight Photonics, Inc.
  (USPTO 7469402, clases 9 y 42) para componentes fotónicos de silicio, otro rubro.
- **Tiendas:** no se encontró ninguna app llamada OpenLight.

Decisión del usuario: **seguir con OpenLight**. Riesgo bajo por ser otro rubro; si hiciera falta
renombrar, los identificadores están concentrados en `Cargo.toml`, `package.json` y
`app/src-tauri/tauri.conf.json`.

El repo de GitHub se crea **privado** y se hace público más adelante.

## 2026-11-22 — Ajustes de wave 0 respecto del plan

- `Region` suma **Singapur** (`openapi-sg.iotbing.com`), que tinytuya soporta.
- `UserFacingError.title` y `.detail` son **claves i18n** (más `params` para interpolar), así el
  backend no tiene textos y la UI traduce.
- `ScreenSource` suma `stop()` con implementación vacía por defecto.
- CI: `macos-13` y `macos-14` no están disponibles en GitHub Actions; macOS corre en `macos-15` (arm64) y `macos-15-intel` (x64).
- Los DTOs del IPC viven en `openlight_types::ipc` (para que `openlight-core` los use) y los
  comandos en `app/src-tauri/src/ipc.rs`. Los nombres de eventos se exportan a TS como constantes
  (`EVENT_SESSION_STATE`, ...).
- `specta`/`tauri-specta` siguen en release candidate (`2.0.0-rc.28/29`): versiones fijadas con `=`.

## 2026-11-23 — Desvíos de wave 1

- **macOS:** `capture` usa `screencapturekit` 12, que compila un puente en Swift contra el SDK de
  macOS 26. Para compilar hace falta **Xcode 26**; el CI lo elige en los runners de macOS (#6).
- **Linux X11:** se usa **XRender** (el servidor reduce la imagen) en vez de XShm: viajan ~330 KB
  por cuadro, nunca se copia la pantalla entera y no hace falta `unsafe` (#9).
- **Linux Wayland:** PipeWire por FFI con `pipewire-sys`/`libspa-sys`, porque los crates seguros
  no compilan a la vez contra PipeWire 0.3.48 (Ubuntu 22.04) y 1.x (#9).
- **Windows:** reducción con mipmaps en la GPU usando el crate `windows` (el mismo de Tauri), no
  `windows-capture` (#2).
- **Ejemplo `color_promedio`:** uno solo para los tres sistemas, sobre `capture::PlatformCapture`.
