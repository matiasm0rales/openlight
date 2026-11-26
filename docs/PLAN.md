# Plan: OpenLight, app de escritorio en Rust para ambilight con focos Tuya / Smart Life

> Nombre: **OpenLight**, una parodia de Ambilight (que es marca de Philips) con guiño al código abierto. Identificadores: `openlight` en crates, binario y repo; bundle id `io.github.<usuario>.openlight`. La tarea T0.1 revisa que no choque con otro proyecto en crates.io, GitHub o las tiendas.

## Estado del proyecto (2026-11-24)

El resto del documento es el plan original. Esta sección dice en qué quedó cada cosa; los números `#N` son pull requests del repo `matiasm0rales/openlight`, que ya es público.

**Resumen:** las waves 0 a 2 y la integración (I1) están en `main` (PRs #1 a #22) con el CI en verde. La prueba con hardware en macOS (I2) está en curso. La prueba en Windows y Linux (I3) está **postergada**. Siguen la wave 4 y la distribución. Todavía no hay ninguna versión publicada.

### Plataformas

| Sistema | Compila y pasa el CI | Probado con focos reales |
|---|---|---|
| macOS (arm64 y x64) | sí | **sí**: una A60 (protocolo 3.3) y un W509Z2 (3.5) |
| Windows | sí | **no**: pendiente (I3) |
| Linux (X11 y Wayland) | sí | **no**: pendiente (I3) |

### Cambios respecto del plan

- La web de descarga va en Vercel, los instaladores en GitHub Releases y la app se actualiza sola.
- **Sin firma ni notarización de Apple por ahora**: el `.dmg` sale sin firmar. El workflow firma solo si existen los secrets (ver [`release.md`](release.md)).
- El resto de los desvíos técnicos está en [`decisiones.md`](decisiones.md).

### Wave 0 — fundación: hecha

| id | estado | dónde | qué falta |
|---|---|---|---|
| T0.1 | hecha | commit `21dea96`, [`decisiones.md`](decisiones.md) | — |
| T0.2 | hecha | commit `21dea96` | — |
| T0.3 | hecha | commit `21dea96`; el contrato se amplió en #1, #15 y #18 | — |
| T0.4 | hecha | commit `21dea96`; runners `macos-15`, `macos-15-intel`, `windows-latest`, `ubuntu-22.04` | — |
| T0.5 | hecha | commit `21dea96`, `tools/golden/` | — |

### Wave 1 — módulos: hecha (integrada en #10)

| id | estado | PRs | qué falta |
|---|---|---|---|
| A `tuya-proto` | hecha; fuzz de 10 min sin panics | #3 | — |
| B `tuya-sim` | hecha; tinytuya real lo controla en 3.1 a 3.5 | #7 | `contra_sim.py` se corre a mano, no está en el CI |
| C `tuya-cloud` + `tuya-discovery` | hecha | #4 | no quedó registrada la prueba manual de `cargo run -p tuya-cloud --example listar` contra una cuenta real |
| D `color-engine` | hecha; golden en verde y benchmark bajo 0,2 ms por zona | #5 | — |
| E1 captura macOS | hecha; medición de CPU en el PR | #6 | — |
| E2 captura Windows | código hecho | #2 | checklist manual en Windows 11 (CPU y HDR): va con I3 |
| E3 captura Linux | código hecho; X11 probado contra Xvfb | #9 | checklist manual en X11 y GNOME Wayland reales (y KDE/wlroots): va con I3 |
| U UI contra mocks | hecha | #1 (contrato), #8 | las escenas del control manual: la lista está vacía y se eligen desde Smart Life |

### Wave 2 — núcleo y empaquetado: hecha

| id | estado | PRs | qué falta |
|---|---|---|---|
| F `tuya-device` | hecha; 24 h simuladas; arreglos con el foco real | #13, #17, #19 | `device22` no está soportado |
| G `openlight-core` + `secrets` | hecha | #12 | fallback cifrado del llavero (sin llavero del sistema, la app no puede guardar claves) |
| P empaquetado y release | el workflow genera los instaladores de los 3 SO | #11 | publicar el primer tag; probar los instaladores ya instalados; firma de Apple (postergada) |

### Wave 3 — integración

| id | estado | PRs | qué falta |
|---|---|---|---|
| I1 app real (IPC, tray, autostart, permisos, calibración) | hecha; además rediseño, logo y selector de idioma | #14, #15, #16, #21, #22 | ver "Pendientes detectados en el código" |
| I2 hardware real en macOS | **en curso**: el usuario usa la app en el día a día | arreglos: #17, #18, #19, #20 | completar el checklist de 4.3; hallazgos en [`i2-hallazgos.md`](i2-hallazgos.md) |
| I3 Windows y Linux | **pendiente y postergada** | — | todo el checklist de 4.3 en cada sistema |

### Wave 4 — terminación y distribución

| id | estado | qué falta |
|---|---|---|
| Q1 cortes de conexión | pendiente; el logger (#20) y `LinkHealth` (#13) ya dan los datos | medir motivo y frecuencia en el foco real y ajustar los valores por defecto |
| Q2 UX, redacción, accesibilidad y capturas | pendiente (hubo mejoras en #16 y #22) | revisión completa y capturas reales del portal de Tuya |
| Q3 documentación | hecha: README, [`guia.md`](guia.md), [`errores.md`](errores.md), [`protocolo.md`](protocolo.md), [`CONTRIBUTING.md`](../CONTRIBUTING.md) | sumar la URL de la web cuando exista |
| Q4 seguridad | hecha: secretos, permisos por ventana, CSP, dependencias, updater y red ([`seguridad.md`](seguridad.md)) | los pendientes del informe; primero, la clave del updater |
| Web en Vercel | en curso | — |
| Updater automático | en curso; el plugin y el workflow ya están (#11) | interfaz de actualización, permiso `updater:default` y clave pública del updater |
| Primera Release | pendiente | tag, borrador revisado y publicado |

### Pendientes detectados en el código

Salieron de leer el código para escribir la documentación; no tienen tarea asignada en el plan:

- Los cambios de **ajustes, perfil activo y zonas** valen recién al detener y volver a arrancar el efecto, pero la pantalla de Ajustes dice que se ven en el foco mientras se mueve el control.
- La **prueba A/B** de la calibración no puede comparar: pide el efecto andando, que no puede tomar el foco que se está calibrando, y las variantes no se aplican en caliente.
- El **control manual** avisa que el efecto «va a pisar lo que elijas», pero con el efecto andando la app rechaza el comando con `device_busy`.
- Los **atajos de los perfiles** se guardan y se muestran, pero no están conectados como atajos de teclado.
- Falta la **integración con la suspensión del sistema** (`NativePower`): la sesión sabe restaurar antes de dormir, pero la app no le avisa.
- El texto de `storage` habla de espacio en disco, y el mismo código sale cuando falla el llavero.
- El texto de `device_busy` habla de Smart Life, y el mismo código sale cuando es la propia app la que está usando el foco (efecto, calibración o control manual).
- Con un sistema en un idioma sin traducción, la interfaz sale en español y el menú del tray en inglés (#22).

## 0. Contexto

Hoy existe un "mini programita" en Python (`~/Documents/Open RGB`):
- `ambilight.py` (620 líneas): captura el monitor, saca el color dominante, lo calibra y lo suaviza, y se lo manda a un foco Tuya por la red local con tinytuya. Reconecta, manda señales de vida, recupera el estado original del foco y se pausa cuando no hay pantalla.
- `calibrar.py`: página web local con 12 tonos donde se ajusta el foco a mano. Genera `calibracion.json`.
- `test_ambilight.py`: unas 60 pruebas sin hardware. Son la especificación del comportamiento que hay que conservar.
- `Ambilight.app`: un applet de AppleScript que lanza el script. La configuración inicial es manual: cuenta en iot.tuya.com, proyecto Cloud, vincular Smart Life con QR, `tinytuya wizard` y editar `config.json` a mano.

**Problemas:** solo sirve para un foco, una Mac y una persona que sepa usar la terminal. Los permisos de macOS se pierden cada vez que se recompila. Cuando la clave cambia (error 914) hay que correr el wizard de nuevo. El log muestra **272 cortes de conexión**, todos seguidos de reconexión: funciona, pero hay algo que entender.

**Objetivo:** una app de escritorio pública y de código abierto, hecha en Rust con Tauri 2, para macOS, Windows y Linux:
1. Asistente paso a paso (stepper) que lleva de cero a "funcionando" sin terminal, incluido el portal de Tuya.
2. Cualquier foco Tuya/Smart Life, con las capacidades detectadas del esquema de DPs (no DPs fijos en el código).
3. **Varios focos con zonas**: cada uno sigue una región de la pantalla.
4. Todos los parámetros configurables desde la interfaz, cada uno con su explicación, más perfiles.
5. Calibración integrada por foco, control manual, diagnóstico y renovación de clave con un clic.
6. La misma calidad de color que el script actual, garantizada con pruebas "golden" generadas por el propio script Python.

**Decisiones tomadas con el usuario:** Tauri + interfaz web (Svelte) · varios focos con zonas · los 3 sistemas en la v1 · pública y de código abierto (GitHub, releases firmados, notarización de Apple).

---

## 1. Arquitectura

### 1.1 Repositorio (nuevo, separado del script actual)
`~/Documents/openlight/`, con `git init` propio y este plan en `docs/PLAN.md`. Está fuera de `~/Documents/Open RGB` para que los archivos con claves de esa carpeta (`devices.json`, `tuya-raw.json`, `snapshot.json`) nunca terminen en el repo público. El script Python queda intacto en `~/Documents/Open RGB` como referencia y como generador de fixtures. Los agentes lo leen por ruta absoluta y no escriben ahí.

```
openlight/
├─ Cargo.toml                    # workspace
├─ crates/
│  ├─ openlight-types/             # CONTRATOS: tipos, traits, errores, catálogo de ajustes (wave 0)
│  ├─ tuya-proto/                # framing + cripto 3.1/3.3/3.4/3.5, sin IO
│  ├─ tuya-sim/                  # foco simulado (servidor TCP + UDP) para pruebas y para la UI
│  ├─ tuya-device/               # actor async: conexión, sesión, latidos, reconexión, DPs → capacidades
│  ├─ tuya-cloud/                # cliente OpenAPI: firma, token, dispositivos, claves, mapeos, regiones
│  ├─ tuya-discovery/            # escucha UDP 6666/6667/7000 y descifra broadcasts
│  ├─ capture/                   # trait ScreenSource + backends macOS / Windows / Linux
│  ├─ color-engine/              # port 1:1 de la matemática de color (funciones puras)
│  ├─ openlight-core/              # runtime: pipeline, zonas, sesión, perfiles, persistencia, restauración
│  └─ secrets/                   # llavero del SO (keyring) con fallback cifrado
├─ app/
│  ├─ src-tauri/                 # comandos/eventos Tauri, tray, autostart, permisos, ventanas
│  └─ ui/                        # Svelte 5 + TS + Vite: stepper, panel, ajustes, calibración, diagnóstico
├─ tools/golden/                 # scripts Python que generan fixtures desde ambilight.py y tinytuya
├─ docs/                         # arquitectura, protocolo, guía Tuya, guía de contribución para agentes
└─ .github/workflows/            # CI matrix + release
```

### 1.2 Flujo en tiempo de ejecución

```
 ScreenSource (hilo bloqueante por monitor)
     │  frame reducido (~96×54 BGRA) vía canal watch (solo el último)
     ▼
 ZoneSampler ── recorta N zonas por foco ──► ColorEngine (por zona)
                                               │ (h,s,luz)
                                               ▼
                                     objetivo_del_foco + Calibración(foco)
                                               ▼
                                         Suavizador(foco)
                                               ▼
                               RateLimiter + umbral (por foco)
                                               ▼
                        DeviceActor (tokio task por foco) ── TCP ──► foco
                                               │
                         eventos (≤10 Hz) ──► Tauri ──► UI
```
- Cada foco tiene su propio actor, su calibración, su suavizador y su límite de envíos. Que un foco se caiga no frena a los demás.
- El canal hacia el actor **coalesce**: si llega un color nuevo antes de mandar el anterior, el anterior se descarta.
- Estados de sesión: `Idle → Conectando → Activo ⇄ EnPausa(sin pantalla) → Restaurando → Idle`, más `Error(código)` por foco.

### 1.3 Tecnologías
| Área | Elección | Motivo |
|---|---|---|
| App | Tauri 2 + plugins `single-instance`, `autostart`, `updater`, `opener`, `log`, `store` | multiplataforma, instalador chico, tray nativo |
| UI | Svelte 5 + TypeScript + Vite, `tauri-specta` para generar los tipos TS de los comandos | stepper rico y tipado de punta a punta |
| Async | tokio | actores de dispositivo, timers |
| Cripto | `aes` + `ecb`, `aes-gcm`, `hmac`, `sha2`, `md-5`, `crc32fast`, `rand` | protocolos 3.1–3.5 |
| HTTP | `reqwest` (rustls) | Tuya OpenAPI |
| Captura macOS | ScreenCaptureKit (`screencapturekit` crate) con salida escalada por la GPU | bajo consumo; CGDisplayCreateImage está obsoleto |
| Captura Windows | Windows Graphics Capture / DXGI (`windows-capture`) | eficiente, soporta HDR |
| Captura Linux | X11: `xcap`/x11rb (XShm) · Wayland: portal ScreenCast (`ashpd`) + PipeWire, con `restore_token` | cubre los dos servidores gráficos |
| Secretos | `keyring` (Keychain / Credential Manager / Secret Service) + fallback cifrado | la clave local y el Access Secret no van en JSON |
| Config | JSON versionado en el directorio de datos de la app (`directories`), con migraciones | |
| Logs | `tracing` + archivo rotativo + exportación de diagnóstico sin secretos | |
| Licencia | MIT/Apache-2.0. Se da crédito a tinytuya (MIT) en `docs/protocolo.md` | |

---

## 2. Contratos (crate `openlight-types`): lo primero y lo único compartido

Las waves paralelas dependen solo de esto. Cualquier cambio pasa por el agente integrador.

```rust
// Color
pub struct Hsv { pub h: f32, pub s: f32, pub v: f32 }          // 0..1
pub struct ScreenColor { pub h: Option<f32>, pub s: f32, pub luz: f32 }

// Dispositivo
pub struct DeviceId(pub String);
pub enum ProtocolVersion { V31, V33, V34, V35 }
pub struct DeviceCreds { pub id: DeviceId, pub local_key: Secret<String>, pub version: ProtocolVersion, pub ip: Option<IpAddr> }
pub enum ColourFormat { V1Hex14, V2Hsv12 }                      // "rrggbb0hhhssvv" | "hhhhssssvvvv"
pub struct Capabilities {
    pub dp_switch: String, pub dp_mode: Option<String>, pub dp_bright: Option<DpRange>,
    pub dp_temp: Option<DpRange>, pub dp_colour: Option<(String, ColourFormat)>,
    pub dp_control: Option<String>,                              // control_data (DP 28 en tu foco)
    pub modes: Vec<String>,                                      // white/colour/scene/music
}
pub enum SendChannel { Colour, ControlDirect, ControlGradual }  // "24" | "directo" | "gradual"

#[async_trait] pub trait LightDevice: Send {
    async fn status(&mut self) -> Result<Dps, DeviceError>;
    async fn set_dps(&mut self, dps: Dps, wait: bool) -> Result<(), DeviceError>;
    async fn heartbeat(&mut self) -> Result<(), DeviceError>;
    fn health(&self) -> LinkHealth;                               // rtt, bytes, reconexiones, motivo del último corte
}

// Pantalla
pub struct DisplayInfo { pub id: DisplayId, pub name: String, pub width: u32, pub height: u32, pub builtin: bool, pub primary: bool }
pub struct Frame { pub width: u32, pub height: u32, pub bgra: Arc<[u8]> }
pub trait ScreenSource: Send {
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    fn start(&mut self, display: DisplayId, target: (u32, u32), fps: f32) -> Result<watch::Receiver<Option<Frame>>, CaptureError>;
    fn permission(&self) -> PermissionState;                     // Granted / Denied / NotDetermined / NotApplicable
    fn request_permission(&self) -> PermissionState;
}

// Zonas
pub struct Zone { pub display: DisplayId, pub rect: NormRect /* x,y,w,h 0..1 */, pub preset: ZonePreset }
pub enum ZonePreset { Full, Left, Right, Top, Bottom, Center, Custom }

// Nube
#[async_trait] pub trait TuyaCloud {
    async fn validate(&self) -> Result<CloudAccount, CloudError>;   // token + uid
    async fn devices(&self) -> Result<Vec<CloudDevice>, CloudError>; // con local_key, product_id, categoría
    async fn mapping(&self, device: &DeviceId) -> Result<DpMapping, CloudError>;
}
pub enum Region { Us, UsEast, Eu, EuWest, China, India }        // base URL de cada una

// Errores visibles: cada uno con código estable, título, explicación y acciones
pub struct UserFacingError { pub code: ErrorCode, pub title: String, pub detail: String, pub actions: Vec<FixAction> }
pub enum FixAction { OpenSettings(SystemPane), RenewKey, Retry, OpenUrl(String), ShowHelp(HelpTopic) }
```

**Catálogo de ajustes** (`settings.rs`): una sola fuente de verdad con id, tipo, valor por defecto, rango, paso, unidad, nivel (básico/avanzado), clave i18n del título y de la explicación. La UI genera los controles desde acá y el core valida contra acá. Los valores por defecto se toman del script actual:

| id | defecto | rango | nivel | qué hace (texto de ayuda) |
|---|---|---|---|---|
| `color_mode` | dominante | dominante/promedio | básico | el color que más se ve, o la mezcla de toda la zona |
| `saturation` | 1.3 | 0.5–2.5 | básico | realce de colores |
| `brightness_min` | 0.05 | 0–1 | básico | brillo con pantalla oscura |
| `brightness_max` | 1.0 | 0–1 | básico | tope de brillo |
| `smoothing_ms` | 300 | 0–2000 | básico | más alto = transiciones más lentas |
| `send_channel` | Colour (24) | Colour/ControlDirect/ControlGradual | avanzado | canal del foco |
| `capture_fps` | 20 | 5–60 | avanzado | capturas por segundo |
| `max_sends_per_s` | 8 | 1–20 | avanzado | comandos por segundo (bajalo si el foco se traba) |
| `send_threshold` | 0.01 | 0–0.1 | avanzado | cambios más chicos no se mandan |
| `black_bar` | 0.06 | 0–0.3 | avanzado | brillo por debajo del cual una fila/columna es banda negra |
| `grey_threshold` | 0.02 | 0–0.2 | avanzado | saturación por debajo de la cual la pantalla es gris |
| `dark_screen` | 0.12 | 0–0.5 | avanzado | por debajo de esto se conserva el último color |
| `hue_stickiness` | 1.4 | 1–3 | avanzado | evita saltar entre dos colores parecidos |
| `no_screen_wait_s` | 20 | 5–300 | avanzado | sin pantalla durante este tiempo, el foco vuelve a su estado |
| `heartbeat_s` | 10 | 3–30 | experto | señal de vida |
| `dead_after_s` | 30 | 5–120 | experto | sin respuesta durante este tiempo, se reconecta |
| `retry_every_s` | 3 | 1–30 | experto | entre intentos de reconexión |

Cada foco puede pisar cualquier ajuste (por ejemplo, otro brillo máximo). Los **perfiles** (Películas, Juegos, Trabajo, propios) guardan un conjunto de valores y se cambian desde el tray.

**Contrato IPC (Tauri)**: lista cerrada de comandos y eventos en `app/src-tauri/src/ipc.rs`, con los tipos TS generados en `app/ui/src/lib/bindings.ts`.
- Comandos: `onboarding_state`, `check_permissions`, `open_system_settings`, `cloud_validate`, `cloud_list_devices`, `discover_lan`, `device_add`, `device_remove`, `device_test(pattern)`, `device_manual(set)`, `displays_list`, `display_preview`, `zones_set`, `settings_get/set/reset`, `profiles_*`, `session_start/stop`, `calibration_start/set/save/cancel`, `renew_key`, `import_legacy(folder)`, `diagnostics_export`.
- Eventos: `session://state`, `device://health`, `color://live` (pantalla vs. enviado, por foco, ≤10 Hz), `error://raised`, `discovery://found`.

---

## 3. Especificación por módulo

### 3.1 `tuya-proto` (sin IO, 100% testeable)
- **3.1/3.3/3.4**: prefijo `0x000055AA`, seq u32, cmd u32, len u32, [retcode u32 en las respuestas], payload, CRC32 (3.1–3.3) o HMAC-SHA256 (3.4), sufijo `0x0000AA55`.
- **3.5**: prefijo `0x00006699`, u16 0, seq, cmd, len, IV de 12 bytes, ciphertext AES-128-GCM, tag de 16 bytes, sufijo `0x00009966`. El AAD es el encabezado.
- Cifrado: 3.3 usa AES-128-ECB + PKCS7 con la local_key y el encabezado de versión `"3.3"+12×0` en los comandos que no son consultas. 3.4/3.5 negocian clave de sesión: cmd 3 (nonce local) → cmd 4 (nonce remoto + HMAC) → cmd 5 (HMAC del remoto). La clave de sesión es `nonce_local XOR nonce_remoto` cifrado con la local_key (ECB en 3.4; GCM con iv = nonce_local[..12] en 3.5).
- Comandos: CONTROL 7 / CONTROL_NEW 13, STATUS 8, HEART_BEAT 9, DP_QUERY 10 / DP_QUERY_NEW 16, UPDATEDPS 18. En 3.4+ el payload es `{"protocol":5,"t":…,"data":{"dps":…}}`.
- **Referencia obligatoria:** `.venv/lib/python3.14/site-packages/tinytuya/core/` (`message_helper.py`, `crypto_helper.py`, `XenonDevice.py`, `command_types.py`). Cada detalle se verifica contra ese código. No se escribe nada de memoria.
- Decodificador de stream incremental: acepta bytes parciales y varios frames juntos, y resincroniza con el prefijo si hay basura.

### 3.2 `tuya-sim`
Foco falso que habla 3.1/3.3/3.4/3.5 por TCP 6668 y emite broadcasts UDP. Tiene DPs configurables (perfil v1 o v2) y fallas inyectables: cerrar el socket cada N s, no responder, rechazar la clave (914), latencia y límite de comandos por segundo. Sirve para las pruebas de integración, para desarrollar la UI sin hardware y para reproducir los cortes del log.

### 3.3 `tuya-device`
- Actor con `mpsc` de comandos y coalescencia de colores. Socket persistente, latido cada `heartbeat_s`, reconexión con backoff y lectura continua de respuestas para que el búfer no se llene (como `_leer_respuestas`).
- Al reconectar vuelve a poner `switch=true, mode=colour` si la sesión está activa. Es el mismo comportamiento que `_al_reconectar`, cubierto por pruebas.
- Distingue los motivos de corte (EOF del foco, timeout, error de IO, 914) y los registra en `LinkHealth`. Así se investigan los 272 cortes.
- `Capabilities::from_mapping(DpMapping)` usa los códigos (`switch_led`, `work_mode`, `bright_value(_v2)`, `temp_value(_v2)`, `colour_data(_v2)`, `control_data`), con una heurística de respaldo por DPs presentes (1–5 en focos viejos, 20–28 en los nuevos).
- Codificación del color: `V2Hsv12` → `%04x%04x%04x` (h 0–360, s/v 0–1000). `V1Hex14` → rrggbb + 0hhh + ss + vv (0–255). `control_data` → `modo + hsv + "00000000"`, exactamente como `valor_color()`. Nunca brillo 0 (mínimo 0.01).
- Restauración: guarda el estado original antes de tocar el foco y lo persiste. Si el programa murió sin restaurar y el foco sigue en `colour`, vale lo guardado (`elegir_original`).
- Búsqueda de IP: primero la IP guardada, después discovery UDP. Si da 914 con la IP guardada, primero busca la IP de nuevo y solo después culpa a la clave (igual que `_esperar_foco`).

### 3.4 `tuya-cloud`
- Firma HMAC-SHA256 en mayúsculas: `client_id + [access_token] + t + nonce + stringToSign`, con `stringToSign = METHOD\nsha256(body)\n\nurl_con_query_ordenada`. Token con `GET /v1.0/token?grant_type=1` y refresco automático.
- Dispositivos y claves: los mismos endpoints que usa tinytuya (`Cloud.py`: `_get_all_devices` con `/v1.0/iot-01/associated-users/devices` y paginado `last_row_key`; `getmapping`). Se verifican contra `.venv/.../tinytuya/Cloud.py` y `wizard.py`.
- Las 6 regiones con su URL base. Si las credenciales dan "permiso denegado", se prueban las demás regiones y se sugiere la correcta.
- Tabla de errores → `UserFacingError` en lenguaje humano: firma inválida (Access Secret mal copiado), token inválido, sin permiso (cuenta de Smart Life no vinculada o región equivocada) y servicio no autorizado o prueba de IoT Core vencida (con cómo extenderla). Los códigos exactos se confirman con respuestas reales y quedan en fixtures.
- La nube solo hace falta al configurar y al renovar la clave. El uso diario es 100% local.

### 3.5 `tuya-discovery`
Escucha UDP 6666 (texto plano), 6667 (cifrado con `md5("yGAdlopoPVldABfn")`) y 7000 (3.5, y envía `REQ_DEVINFO 0x25`), como `tinytuya/core/udp_helper.py` y `scanner.py`. Devuelve `{id, ip, version, product_key}` y cruza los resultados con los dispositivos de la nube.

### 3.6 `capture`
- macOS: ScreenCaptureKit con `SCStreamConfiguration` a ~96×54 y `minimumFrameInterval` = 1/fps. Permiso con `CGPreflightScreenCaptureAccess`/`CGRequestScreenCaptureAccess`. Reacciona a monitores que aparecen, se van o se duermen (hoy lo hace `Captura._revisar`).
- Windows: Graphics Capture con reducción en GPU, o reducción por stride en CPU. HDR se convierte a SDR antes de medir.
- Linux: X11 con XShm. En Wayland, portal ScreenCast con `persist_mode=2` y `restore_token` guardado, para no pedir permiso en cada sesión.
- Selección por defecto: el primer monitor externo, y si no hay, el integrado. Vuelve al externo cuando aparece (`elegir_monitor`).
- Exclusión: las ventanas propias (calibración) no se capturan cuando corresponde.

### 3.7 `color-engine` (port fiel, sin cambios de comportamiento en la v1)
Funciones: `color_de_pantalla` (incluye `zona_dominante` con histograma de 36 bins pesado por `mx²·sat`, suavizado circular y ventaja del tono previo), `objetivo_del_foco`, `diferencia`, `Calibracion` (interpolación circular con cierre en 360°), `Suavizador` (constante de tiempo adaptativa, camino corto del tono, salto directo desde casi blanco) y `separacion_tonos`. Las constantes salen del catálogo de ajustes.

### 3.8 `openlight-core`
- `Session`: arma el pipeline. Una captura por monitor usado. N zonas por foco, desde una o varias capturas.
- Pausa sin pantalla: después de `no_screen_wait_s`, devuelve cada foco a su estado y retoma cuando vuelve la pantalla.
- Eventos del SO: antes de suspender, restaura los focos. Al despertar, reanuda (macOS NSWorkspace, Windows WM_POWERBROADCAST, Linux logind `PrepareForSleep`).
- Persistencia: `config.json` (v1, con migraciones), `devices` (sin claves; las claves van al llavero), `calibration/<device>.json`, `state.json` (última IP y original por foco).
- Una sola instancia (plugin de Tauri) y un lock por foco, para que calibración y sesión no se peleen.
- **Importar el setup actual**: lee `devices.json`, `config.json` y `calibracion.json` de una carpeta. Las claves van al llavero y la calibración al foco que corresponde. Tu configuración pasa en un clic.

### 3.9 UI (Svelte)
**Principios:** cada pantalla dice qué está pasando y qué hacer. Ningún error queda sin acción sugerida. Accesible: teclado completo, ARIA, contraste AA, respeta "reducir movimiento" y tema claro/oscuro. i18n en español (voseo, como el script) e inglés, con los textos en archivos separados.

**A. Asistente inicial (stepper)** — barra de progreso; se puede volver atrás, pausar y retomar (el estado persiste); cada paso valida antes de habilitar "Siguiente".
1. **Bienvenida**: qué hace la app, qué necesitás (focos ya agregados en Smart Life, la compu en la misma Wi-Fi, ~15 min) y por qué hace falta una cuenta de desarrollador de Tuya (para leer la clave local del foco una sola vez; después todo funciona sin nube). Atajo: **"Ya tengo devices.json"** → importar.
2. **Permisos del sistema** (según el SO): estado en vivo de Grabación de pantalla y Red local (macOS), aviso de firewall (Windows) y portal (Wayland). Botón "Abrir Ajustes" en el panel exacto y re-chequeo automático. Explica por qué macOS pide reabrir la app.
3. **Cuenta en Tuya Developer**: botón que abre iot.tuya.com, guía ilustrada y casilla "Ya tengo cuenta".
4. **Crear el proyecto Cloud**: Cloud → Development → Create Cloud Project. Industry y Development Method: *Smart Home*. Selector de "¿dónde creaste tu cuenta de Smart Life?" → muestra qué Data Center elegir. Explica la prueba de IoT Core y su vencimiento.
5. **Vincular Smart Life**: Devices → Link App Account → Add App Account → QR. En el celular: Smart Life → Yo → ícono de escanear. Ilustración del celular.
6. **Pegar Access ID y Access Secret**: los dos campos, botón "Probar" y validación en vivo con error humano y arreglo concreto. Si la región está mal, se detecta y se ofrece la correcta.
7. **Elegir focos**: lista de la nube cruzada con el discovery de la red ("encontrado en tu red", IP, protocolo). Filtra las luces, muestra capacidades y avisa si un dispositivo no soporta color. Permite elegir varios.
8. **Probar cada foco**: conexión local y secuencia rojo/verde/azul. "¿Lo viste?" Sí → sigue. No → diagnóstico guiado (otra Wi-Fi, app Smart Life abierta ocupando la conexión, clave vieja, foco apagado del interruptor).
9. **Monitor y zonas**: miniaturas de los monitores con vista previa en vivo y presets (toda, izquierda, derecha, arriba, abajo, centro) o rectángulo arrastrable por foco. Muestra el color calculado de cada zona en tiempo real.
10. **Calibrar (recomendado, se puede saltear)**: entra a la calibración (C).
11. **Listo**: "Arrancar", "Abrir al iniciar sesión" y dónde queda el ícono del tray.

**B. Panel principal**: interruptor grande, perfil activo y una tarjeta por foco (conectado/reconectando, color de pantalla vs. color enviado, latencia, cortes en la última hora, brillo rápido). Accesos a control manual y calibración.

**C. Calibración** (reemplaza a `calibrar.py`): ventana a pantalla completa en el monitor elegido, 12 tonos, sliders de tono (±90°) y saturación (0.3–1.5) que mueven el foco en vivo, mismas teclas (←/→, ↑/↓, Enter). Una tabla por foco. Modo rápido de 6 tonos. **Prueba A/B guiada** ("¿cuál se parece más?") para elegir `send_channel` y `color_mode`, que resuelve lo que quedó pendiente con el script.

**D. Ajustes**: generados desde el catálogo. Básico / Avanzado / Experto, cada control con explicación, valor por defecto y "restaurar". Vista previa en vivo mientras se ajusta. Perfiles: crear, duplicar, renombrar y asignar atajo.

**E. Control manual**: selector de color, blanco con temperatura, brillo y escenas del foco si las tiene.

**F. Dispositivos**: agregar o quitar focos, **"Renovar clave"** (un clic, usa las credenciales guardadas; también lo ofrece el error 914), cambiar IP fija o automática, ver protocolo y DPs.

**G. Diagnóstico**: gráfico de fps capturados y envíos por segundo, salud de cada enlace con el motivo del último corte, visor de log y "Exportar diagnóstico" (zip sin secretos).

**Tray / barra de menú**: encender/apagar, perfil, foco por foco, abrir y salir (salir restaura los focos).

### 3.10 Empaquetado y distribución
- macOS: `.dmg` universal (arm64 + x64), firmado con Developer ID y notarizado. Info.plist con `NSLocalNetworkUsageDescription` y `NSScreenCaptureUsageDescription` (hoy se agregan a mano en `crear_app.sh`). Con la misma identidad de firma, los permisos sobreviven a las actualizaciones, que es el problema que tiene hoy el applet.
- Windows: MSI/NSIS firmado (o sin firmar con aviso en la v1) y regla de firewall para UDP discovery.
- Linux: AppImage, `.deb` y `.rpm`. Flatpak queda para después.
- Actualizaciones automáticas con `tauri-plugin-updater` desde GitHub Releases, firmadas.

---

## 4. Plan de ejecución multi-agente

### 4.1 Reglas para todos los agentes (`docs/AGENTS.md`, lo escribe T0)
- Cada tarea en su **worktree y rama** `feat/<id-tarea>`, con PR a `main` y CI verde para mergear.
- Cada tarea **es dueña de sus carpetas**. No edita las de otra. Un cambio en `openlight-types` o en el contrato IPC se pide al integrador (I-0) con un PR chico y separado.
- Prohibido: commitear `devices.json`, claves o capturas reales. Las fixtures de la nube se sanean con un script (`tools/sanitize.py`).
- Antes de abrir el PR: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test -p <crate>` (y `pnpm test` en la UI).
- Todo comportamiento portado de Python lleva la prueba equivalente con el mismo nombre traducido, y un comentario `// port: test_ambilight.py::<nombre>`.
- Los detalles del protocolo y de la nube se verifican contra el código de tinytuya en `.venv`, nunca de memoria.

### 4.2 Waves y tareas

**Wave 0 — fundación (1 agente, secuencial, bloquea todo)**
| id | tarea | entregable / criterio de aceptación |
|---|---|---|
| T0.1 | Revisar que "OpenLight" esté libre (crates.io, GitHub, tiendas, marcas parecidas) y crear el repo `openlight` | repo creado; si hay choque, avisar antes de seguir |
| T0.2 | Scaffold: workspace, `app/` Tauri 2 + Svelte, licencias, README, `docs/AGENTS.md` | `cargo build` y `pnpm tauri dev` abren una ventana vacía en los 3 SO |
| T0.3 | `openlight-types` completo (sección 2) + catálogo de ajustes + `ipc.rs` con stubs + bindings TS | compila, documentado, con pruebas de que los defaults están dentro de sus rangos |
| T0.4 | CI: matrix macos-14 (arm64), macos-13 (x64), windows-latest, ubuntu-22.04 con fmt/clippy/test/build | pipeline verde |
| T0.5 | `tools/golden/`: scripts que generan fixtures desde `ambilight.py` (color) y desde tinytuya (frames con nonce/IV fijos parcheando `os.urandom`) | `fixtures/color/*.json` y `fixtures/proto/*.json` commiteados |

**Wave 1 — en paralelo (hasta 7 agentes), solo dependen de wave 0**
| id | tarea | dueño de | acepta cuando |
|---|---|---|---|
| A | `tuya-proto` | `crates/tuya-proto` | encode/decode de los 4 protocolos coincide byte a byte con las fixtures de tinytuya; fuzz del decoder (`cargo fuzz`, 10 min sin panics) |
| B | `tuya-sim` | `crates/tuya-sim` | **tinytuya real** (desde `.venv`) controla el simulador en 3.3, 3.4 y 3.5 (script `tools/golden/contra_sim.py`) |
| C | `tuya-cloud` + `tuya-discovery` | esos 2 crates | pruebas con `wiremock` sobre fixtures saneadas; descifra broadcasts de fixtures; ejemplo `cargo run --example listar` contra la cuenta real (manual) |
| D | `color-engine` | `crates/color-engine` | las ~35 pruebas de color, objetivo, calibración, diferencia y suavizador portadas y en verde; golden con tolerancia ≤0.5° de tono y ≤0.005 en s/luz; benchmark (criterion) <0.2 ms por zona a 96×54 |
| E1 | Captura macOS | `capture/src/macos` | ejemplo que imprime el color promedio a 20 fps con <3% de CPU en Apple Silicon; detecta conexión y desconexión de monitores |
| E2 | Captura Windows | `capture/src/windows` | ídem en Windows 11, incluido un monitor HDR si hay |
| E3 | Captura Linux | `capture/src/linux` | ídem en X11 y en GNOME Wayland (portal + restore token) |
| U | UI completa contra mocks | `app/ui` | todas las pantallas de 3.9 andan con `@tauri-apps/api/mocks` + datos falsos; Vitest + Playwright en verde; auditoría de accesibilidad (axe) sin errores serios; textos en es y en |

**Wave 2 — depende de A, B y D**
| id | tarea | dueño de | acepta cuando |
|---|---|---|---|
| F | `tuya-device` (actor) | `crates/tuya-device` | pruebas de `FocoArranque`, `FocoConexion` y `FocoRestaurar` portadas, corriendo contra `tuya-sim` con fallas inyectadas; 24 h simuladas sin fugas ni bloqueos |
| G | `openlight-core` + `secrets` | esos crates | pipeline con 2 focos simulados y una captura falsa (video sintético) que respeta zonas, límites, pausa y restauración; importar el setup legado funciona con una copia de la carpeta actual |
| P | Empaquetado y release | `.github/workflows/release.yml`, `app/src-tauri/tauri.conf.json` (sección bundle), íconos | un tag `v0.1.0-rc1` produce instaladores de los 3 SO; macOS firmado y notarizado (si ya están las credenciales de Apple) |

**Wave 3 — integración (1 agente integrador, I-0)**
| id | tarea | acepta cuando |
|---|---|---|
| I1 | Implementar los comandos y eventos IPC reales en `src-tauri`, tray, autostart, permisos por SO, ventana de calibración a pantalla completa en el monitor elegido | la UI deja de usar mocks; el flujo completo anda contra `tuya-sim` |
| I2 | Prueba end-to-end con hardware real (tus 2 focos) en macOS | checklist de 4.3 completo |
| I3 | Lo mismo en Windows y Linux (máquina o VM con red puenteada) | checklist completo en cada uno |

**Wave 4 — terminación (en paralelo)**
| id | tarea |
|---|---|
| Q1 | Investigar los cortes de conexión con `LinkHealth` en tu foco real: motivo, frecuencia, relación con `max_sends_per_s` y el latido. Ajustar los defaults si conviene |
| Q2 | Revisión de UX y redacción (skill `design:ux-copy`), auditoría de accesibilidad, capturas para la guía de Tuya |
| Q3 | Documentación: README público, guía de usuario, FAQ de errores, `docs/protocolo.md`, CONTRIBUTING |
| Q4 | Revisión de seguridad: secretos fuera de logs y exportaciones, permisos mínimos de Tauri (capabilities), CSP |

**Paralelismo máximo:** wave 1 con 8 agentes (A, B, C, D, E1, E2, E3, U). E2 y E3 necesitan runners de Windows y Linux, o el CI como entorno de prueba.

### 4.3 Verificación final (definición de terminado de la v1)
1. `cargo test --workspace` y `pnpm test` en verde en los 3 SO (CI).
2. Golden: el motor en Rust reproduce los colores de `ambilight.py` sobre las fixtures dentro de la tolerancia.
3. Protocolo: tinytuya ↔ `tuya-sim` y el cliente Rust ↔ `tuya-sim` en 3.1, 3.3, 3.4 y 3.5.
4. Hardware, en tu Mac con el W509Z2 (3.5) y la A60 BAW:
   - instalación desde cero con el stepper, sin terminal, en menos de 15 min;
   - "Importar setup existente" desde `~/Documents/Open RGB` deja todo funcionando;
   - dos focos con zonas izquierda y derecha siguiendo un video de prueba;
   - comparación a ojo contra el script Python con el mismo video y la misma calibración: igual o mejor;
   - desenchufar el monitor, dormir la Mac, cortar la Wi-Fi y apagar un foco del interruptor: recupera solo y devuelve los focos a su estado;
   - forzar un 914 (clave vieja): "Renovar clave" lo resuelve en un clic;
   - CPU <5% y RAM <150 MB con 2 focos a 20 fps.
5. Las actualizaciones no piden de nuevo los permisos de macOS.

### 4.4 Riesgos y mitigaciones
| riesgo | mitigación |
|---|---|
| El portal de Tuya cambia sus pantallas | los textos y las imágenes del stepper son contenido editable (`ui/src/content/`), no código; Q2 los actualiza con capturas reales |
| La prueba gratis de IoT Core vence | detectar el error, explicar cómo extenderla; recordar que solo hace falta para renovar claves |
| La app Smart Life ocupa la conexión local del foco | diagnóstico específico en el paso 8 y en el panel |
| Wayland sin portal ScreenCast (compositores raros) | mensaje claro de "no soportado" y fallback a XWayland si existe |
| Local Network de macOS bloquea el discovery UDP | se pide el permiso en el paso 2 antes de buscar; IP manual como plan B |
| Cuesta igualar la calidad del script | el port es fiel y verificado con golden; las mejoras de algoritmo quedan para después de la v1 y siempre detrás de un ajuste |
| Credenciales de Apple y Windows para firmar | P funciona sin ellas (builds sin firmar marcadas como tales); se agregan como secretos del repo cuando estén |

### 4.5 Fuera de alcance de la v1 (backlog)
Modo música (micrófono), escenas automáticas por horario, tiras LED con segmentos (DP de píxeles), Home Assistant y MQTT, app móvil, mejoras del algoritmo de color.

---

## 5. Archivos de referencia del proyecto actual (para los agentes)
- `~/Documents/Open RGB/ambilight.py`: algoritmo, conexión, restauración y monitores (especificación de comportamiento).
- `~/Documents/Open RGB/calibrar.py`: UX y lógica de la calibración (`tabla_inicial`, rangos de los sliders, teclas).
- `~/Documents/Open RGB/test_ambilight.py`: pruebas a portar (clases `ColorDePantalla`, `ObjetivoDelFoco`, `Calibracion`, `Diferencia`, `Suavizador`, `ValorColor`, `ElegirMonitor`, `ElegirOriginal`, `FocoArranque`, `FocoConexion`, `FocoRestaurar`, `Candado`, y el `DispositivoFalso` como modelo de `tuya-sim`).
- `~/Documents/Open RGB/devices.json`: ejemplo real de mapeo de DPs de 2 focos (no se commitea; se sanea para fixtures).
- `~/Documents/Open RGB/.venv/lib/python3.14/site-packages/tinytuya/`: referencia del protocolo (`core/`), de la nube (`Cloud.py`, `wizard.py`) y del discovery (`scanner.py`, `core/udp_helper.py`).
- Entorno: macOS 26.5.1 arm64, Rust 1.100 instalado.
