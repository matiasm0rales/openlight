# Guía para agentes y contribuidores

Reglas para trabajar en OpenLight, sobre todo en paralelo (sección 4 de [`PLAN.md`](PLAN.md)).

## Flujo de trabajo

1. Cada tarea trabaja en **su propio worktree y su rama** `feat/<id-tarea>` (por ejemplo
   `feat/A-tuya-proto`) y abre un PR a `main`. Se mergea solo con el CI en verde.
2. Cada tarea **es dueña de sus carpetas** (tabla de abajo) y no edita las de otra.
3. **Contratos:** cualquier cambio en `crates/openlight-types`, en `app/src-tauri/src/ipc.rs` o en
   `app/ui/src/lib/bindings.ts` se pide al integrador (I-0) con un PR **chico y separado**.
   No se mezcla con el trabajo de la tarea.
4. Antes de abrir el PR, todo esto tiene que pasar:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test -p <tu-crate>
   pnpm test && pnpm check        # si tocaste app/ui
   ```

## Dueños de carpetas

| tarea | carpetas |
|---|---|
| I-0 (integrador) | `crates/openlight-types`, `app/src-tauri/src/ipc.rs`, `app/ui/src/lib/bindings.ts`, `Cargo.toml` raíz, `docs/AGENTS.md` |
| A | `crates/tuya-proto` |
| B | `crates/tuya-sim`, `tools/golden/contra_sim.py` |
| C | `crates/tuya-cloud`, `crates/tuya-discovery` |
| D | `crates/color-engine` |
| E1 / E2 / E3 | `crates/capture/src/macos`, `.../windows`, `.../linux` |
| U | `app/ui` (salvo `bindings.ts`) |
| F | `crates/tuya-device` |
| G | `crates/openlight-core`, `crates/secrets` |
| P | `.github/workflows/release.yml`, sección `bundle` de `app/src-tauri/tauri.conf.json`, `app/src-tauri/icons` |

`crates/capture/src/lib.rs` y `crates/capture/Cargo.toml` son compartidos por E1–E3: cada uno
agrega solo sus líneas (`cfg(target_os = ...)`), y si hay conflicto lo resuelve el integrador.

## Reglas de contenido

- **Prohibido commitear** `devices.json`, claves locales, Access ID/Secret, `tuya-raw.json`,
  `snapshot.json` o capturas de pantalla reales. Las respuestas reales de la nube se sanean con
  `tools/sanitize.py` antes de convertirse en fixtures.
- Los secretos usan `openlight_types::Secret`: no implementa `Serialize`, así que no pueden
  terminar en un JSON ni en el IPC por accidente.
- **Port desde Python:** todo comportamiento portado de `ambilight.py` lleva la prueba equivalente
  de `test_ambilight.py`, con el mismo nombre traducido y el comentario
  `// port: test_ambilight.py::<Clase>::<nombre>`.
- **Protocolo y nube:** cada detalle se verifica contra el código de tinytuya
  (`~/Documents/Open RGB/.venv/lib/python3.14/site-packages/tinytuya/`), nunca de memoria.
  Las fixtures de `fixtures/proto` salen de ese código (ver `tools/golden/`).
- El script original (`~/Documents/Open RGB`) se lee por ruta absoluta. **Nunca se escribe ahí.**
- Textos visibles: nunca en el código de Rust. Se usan claves i18n (`settings.<id>.title`,
  `error.<código>.detail`) que viven en `app/ui/src/lib/i18n/{es,en}.json`. Español con voseo.
  Una prueba verifica que cada ajuste y cada código de error tenga texto en los dos idiomas.

## Contratos clave

- `openlight_types::settings`: el catálogo de ajustes es la única fuente de verdad de defaults,
  rangos, pasos, unidades y niveles. La UI genera los controles desde ahí.
- `openlight_types::error::UserFacingError`: todo error que llega a la UI tiene código estable,
  claves i18n y al menos una acción sugerida.
- `openlight_types::ipc`: DTOs y nombres de eventos. Los comandos están en
  `app/src-tauri/src/ipc.rs`; `cargo test -p openlight` regenera `bindings.ts` y el CI falla si
  el archivo commiteado no coincide.

## Fixtures golden

`tools/golden/` genera, desde el código Python de referencia:

- `fixtures/color/*.json`: entradas y salidas de `color_de_pantalla`, `objetivo_del_foco`,
  `diferencia`, `Calibracion` y `Suavizador` (tarea D: tolerancia ≤0.5° de tono, ≤0.005 en s/luz).
- `fixtures/proto/*.json`: frames de tinytuya con nonce e IV fijos (tarea A: igualdad byte a byte).

Para regenerarlas: `tools/golden/README.md`.
