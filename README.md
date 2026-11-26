# OpenLight

Tus focos Tuya / Smart Life siguen los colores de tu pantalla, como un Ambilight casero.
App de escritorio para macOS, Windows y Linux, hecha en Rust con Tauri 2. Gratis y de código
abierto.

> **In English:** OpenLight makes your Tuya / Smart Life bulbs follow the colours on your screen
> (DIY ambilight). It talks to the bulbs over your local network; the Tuya cloud is only needed
> to fetch each bulb's key. Desktop app (Rust + Tauri 2) for macOS, Windows and Linux, in
> Spanish and English. Only macOS has been tested with real hardware so far.
> [More in English](#in-english).

*OpenLight no está afiliado a Philips (Ambilight es marca de Philips) ni a Tuya.*

## Para quién es

Para quien tiene focos de color en la app Smart Life (o Tuya Smart) y quiere que acompañen
películas, juegos o el escritorio, sin comprar hardware nuevo ni usar la terminal.

Necesitás:

- uno o más focos o tiras de luz **con color**, ya agregados en Smart Life;
- la compu y los focos en la **misma red** Wi-Fi o cableada;
- una cuenta gratis en el portal de desarrolladores de Tuya, para leer la clave local de cada
  foco (el asistente te lleva paso a paso, son unos 15 minutos y se hace una sola vez).

## Qué hace

- Asistente que lleva de cero a "funcionando", incluido el portal de Tuya.
- Control **local**: los colores van de tu compu al foco por tu red. La nube de Tuya solo hace
  falta para agregar focos o renovar una clave.
- Protocolos locales 3.1, 3.3, 3.4 y 3.5.
- Varios focos, cada uno siguiendo una zona de la pantalla.
- Calibración por foco, perfiles, control manual y diagnóstico.
- Cada foco vuelve a como estaba cuando detenés el efecto o salís de la app.

Las claves se guardan en el llavero del sistema y la imagen de tu pantalla no sale de tu compu.

## Estado por plataforma

| Sistema | Estado |
|---|---|
| **macOS** 12.3 o más nuevo | **Probado con focos reales**: una A60 (protocolo 3.3) y un W509Z2 (3.5). |
| **Windows** | Compila y pasa el CI. **Todavía no se probó con hardware real.** |
| **Linux** (X11 y Wayland) | Compila y pasa el CI. **Todavía no se probó con hardware real.** |

La prueba con hardware en Windows y Linux (tarea I3 del [plan](docs/PLAN.md)) quedó pendiente y
postergada. Si la probás en alguno de los dos, [contá cómo te fue en un
issue](https://github.com/matiasm0rales/openlight/issues).

## Instalar

La forma normal va a ser bajarla de la web:

> **[PENDIENTE: URL de la web de descarga]**

Los instaladores también se publican en
[GitHub Releases](https://github.com/matiasm0rales/openlight/releases). Todavía no hay ninguna
versión publicada: mientras tanto, podés [compilarla desde el código](#compilar-desde-el-código).

| Sistema | Qué bajar |
|---|---|
| macOS | el `.dmg` universal (Apple Silicon e Intel) |
| Windows | `-setup.exe` (por usuario, no pide permisos de administrador) o `.msi` |
| Linux | `.AppImage`, `.deb` o `.rpm` |

Los instaladores **no están firmados** por ahora, así que macOS y Windows van a mostrar un aviso
la primera vez. Cómo pasarlo, en la [guía](docs/guia.md#instalar).

## Documentación

| Documento | Para qué |
|---|---|
| [Guía de usuario](docs/guia.md) | instalar, configurar, usar, ajustes y desinstalar |
| [Errores frecuentes](docs/errores.md) | qué significa cada error y cómo se arregla |
| [Protocolo local de Tuya](docs/protocolo.md) | cómo habla OpenLight con los focos |
| [Contribuir](CONTRIBUTING.md) | pruebas, convenciones y pull requests |
| [Releases](docs/release.md) | cómo se arman y publican los instaladores |
| [Plan](docs/PLAN.md) y [decisiones](docs/decisiones.md) | diseño, estado y por qué |

## Compilar desde el código

Requisitos:

- Rust estable, 1.90 o más nuevo;
- Node 22 y pnpm (la versión de pnpm está fijada en `package.json`);
- las [dependencias de Tauri](https://tauri.app/start/prerequisites/) de tu sistema;
- en macOS, **Xcode 26** (o sus Command Line Tools): el crate de captura lo necesita;
- en Linux, además, `libpipewire-0.3-dev` y `libclang-dev` (la lista completa para Ubuntu está
  en [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

```bash
pnpm install
```

```bash
pnpm tauri dev
```

Para desarrollar **sin tocar el llavero real ni tu configuración**:

```bash
OPENLIGHT_SECRETS=memory OPENLIGHT_DATA_DIR=/tmp/openlight-dev pnpm tauri dev
```

| Variable | Efecto |
|---|---|
| `OPENLIGHT_SECRETS=memory` | las claves quedan en memoria y se pierden al cerrar la app |
| `OPENLIGHT_DATA_DIR=<carpeta>` | configuración, logs y diagnósticos van a esa carpeta |

En PowerShell:

```powershell
$env:OPENLIGHT_SECRETS="memory"; $env:OPENLIGHT_DATA_DIR="$env:TEMP\openlight-dev"; pnpm tauri dev
```

Las pruebas, el foco simulado y las convenciones están en [CONTRIBUTING.md](CONTRIBUTING.md).

## Estructura del repositorio

| Carpeta | Qué hay |
|---|---|
| `crates/openlight-types` | contratos compartidos: tipos, traits, errores, catálogo de ajustes, DTOs del IPC |
| `crates/tuya-proto` | framing y cripto del protocolo local (sin IO) |
| `crates/tuya-sim` | foco Tuya simulado, para pruebas y desarrollo |
| `crates/tuya-device` | un actor por foco: conexión, latidos, reconexión y restauración |
| `crates/tuya-cloud` | cliente de la OpenAPI de Tuya |
| `crates/tuya-discovery` | búsqueda de focos en la red local (UDP) |
| `crates/capture` | captura de pantalla en macOS, Windows y Linux |
| `crates/color-engine` | matemática de color (port del script original) |
| `crates/openlight-core` | pipeline, zonas, sesión, perfiles, persistencia e importación |
| `crates/secrets` | llavero del sistema |
| `app/src-tauri` | app de Tauri: comandos, tray, permisos, logs y contrato IPC |
| `app/ui` | interfaz en Svelte 5 + TypeScript |
| `tools/` | generadores de fixtures, saneador de respuestas de la nube e íconos |
| `fixtures/` | fixtures "golden" de color, protocolo y nube |
| `docs/` | documentación |

## In English

**What it is.** A desktop app that makes Tuya / Smart Life colour bulbs follow your screen.
Colours go straight from your computer to the bulb over the local network. A free Tuya developer
account is needed once, to read each bulb's local key; the setup assistant walks you through it.
The UI is available in English and Spanish.

**Platforms.** macOS 12.3+ is tested with real bulbs (protocols 3.3 and 3.5). Windows and Linux
build and pass CI but have **not been tested with real hardware** yet.

**Install.** Download page: **[TODO: download website URL]**. Installers will also be on
[GitHub Releases](https://github.com/matiasm0rales/openlight/releases) (none published yet).
They are not code-signed for now.

**Build from source.** Rust 1.90+, Node 22, pnpm and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) (Xcode 26 on macOS), then
`pnpm install && pnpm tauri dev`. Set `OPENLIGHT_SECRETS=memory` and
`OPENLIGHT_DATA_DIR=<folder>` to keep your real keychain and settings untouched.

The rest of the documentation is in Spanish.

## Créditos

El protocolo local de Tuya se implementa siguiendo
[tinytuya](https://github.com/jasonacox/tinytuya) (MIT).

## Licencia

`MIT OR Apache-2.0`: a elección, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE).
