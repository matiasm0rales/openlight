# OpenLight

Tus focos Tuya / Smart Life siguen los colores de tu pantalla, como un Ambilight casero.
App de escritorio para macOS, Windows y Linux, hecha en Rust con Tauri 2. Código abierto.

> **Estado:** en desarrollo (wave 0: fundación). Todavía no hace nada útil; el plan completo
> está en [`docs/PLAN.md`](docs/PLAN.md).

*OpenLight no está afiliado a Philips (Ambilight es marca de Philips) ni a Tuya.*

## Qué va a hacer

- Asistente paso a paso que lleva de cero a "funcionando" sin terminal, incluida la cuenta de Tuya.
- Cualquier foco Tuya / Smart Life (protocolos 3.1, 3.3, 3.4 y 3.5), por la red local.
- Varios focos, cada uno siguiendo una zona de la pantalla.
- Calibración por foco, perfiles, diagnóstico y renovación de clave con un clic.

## Desarrollo

Requisitos: Rust estable (1.90 o más nuevo), Node 20+ y pnpm, y las
[dependencias de Tauri](https://tauri.app/start/prerequisites/) de tu sistema.

```bash
pnpm install          # dependencias de la UI y la CLI de Tauri
pnpm tauri dev        # abre la app con recarga en vivo
cargo test --workspace
pnpm test             # pruebas de la UI (Vitest)
```

Estructura:

| carpeta | qué hay |
|---|---|
| `crates/openlight-types` | contratos compartidos: tipos, traits, errores, catálogo de ajustes, DTOs del IPC |
| `crates/tuya-*` | protocolo local, simulador, actor de dispositivo, nube y discovery de Tuya |
| `crates/capture` | captura de pantalla por sistema operativo |
| `crates/color-engine` | matemática de color (port del script original) |
| `crates/openlight-core` | pipeline, zonas, sesión, perfiles y persistencia |
| `crates/secrets` | llavero del sistema |
| `app/src-tauri` | shell de Tauri y contrato IPC (`src/ipc.rs`) |
| `app/ui` | interfaz en Svelte 5 + TypeScript |
| `tools/golden` | generadores de fixtures desde el script Python y tinytuya |
| `fixtures/` | fixtures "golden" de color y de protocolo |

Para contribuir (personas o agentes), leé [`docs/AGENTS.md`](docs/AGENTS.md).

## Créditos

El protocolo local de Tuya se implementa siguiendo [tinytuya](https://github.com/jasonacox/tinytuya) (MIT).

## Licencia

A elección, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE).
