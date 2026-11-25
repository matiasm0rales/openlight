# Contribuir a OpenLight

Gracias por sumarte. Esta guía dice cómo correr el proyecto, probarlo y mandar cambios. Las
reglas para trabajar en paralelo (personas o agentes) están en
[`docs/AGENTS.md`](docs/AGENTS.md).

Si encontraste un error y no vas a arreglarlo,
[abrí un issue](https://github.com/matiasm0rales/openlight/issues) con el código del error, tu
sistema, el modelo y protocolo del foco y, si podés, el diagnóstico exportado (**revisalo
antes**: trae nombres, ids e IPs locales). Las pruebas en **Windows y Linux con focos reales**
son lo que más falta.

## Preparar el entorno

Los requisitos están en el [README](README.md#compilar-desde-el-código).

```bash
pnpm install
```

Para desarrollar, no toques tu llavero ni tu configuración reales:

```bash
OPENLIGHT_SECRETS=memory OPENLIGHT_DATA_DIR=/tmp/openlight-dev pnpm tauri dev
```

Con el llavero en memoria las claves se pierden al cerrar: la configuración queda en la carpeta,
pero hay que volver a cargar las claves en cada arranque.

El nivel del registro se cambia con `RUST_LOG` (por ejemplo `RUST_LOG=tuya_device=trace`). Solo
la interfaz, contra un backend falso y en el navegador: `pnpm -C app/ui dev`. En Linux,
`OPENLIGHT_CAPTURE=x11` o `=wayland` fuerza el método de captura.

## Pruebas

Antes de abrir un PR tiene que pasar todo esto, que es lo que corre el CI:

```bash
pnpm --filter openlight-ui build
```

(Tauri necesita la interfaz construida para compilar el workspace entero.)

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
pnpm check
```

```bash
pnpm -C app/ui test
```

`pnpm -C app/ui test` corre Vitest y Playwright (usa el puerto 1420). La primera vez hay que
instalar el navegador:

```bash
pnpm -C app/ui exec playwright install chromium
```

Notas:

- Si cambiaste comandos o tipos del IPC, regenerá `app/ui/src/lib/bindings.ts` y commitealo; el
  CI falla si no coincide:

  ```bash
  cargo test -p openlight ipc::tests::exporta_los_bindings
  ```

- Hay pruebas marcadas `#[ignore]` porque necesitan hardware o permisos: las de captura
  (`cargo test -p capture -- --ignored`) y la del llavero
  (`cargo test -p secrets -- --ignored`, que **toca tu llavero real**).
- Si cambiaste el catálogo de ajustes o sus textos, actualizá la guía:

  ```bash
  python3 tools/gen_ajustes.py
  ```

- Si agregás un `ErrorCode`, sumá sus textos en `es.json` y `en.json` y su entrada en
  [`docs/errores.md`](docs/errores.md).

## El foco simulado: `tuya-sim`

Las pruebas de integración no usan hardware: arrancan un foco falso que habla 3.1 a 3.5 por TCP
y se anuncia por UDP. También se puede correr suelto:

```bash
cargo run -p tuya-sim --example foco -- --version 3.5
```

Sirve para provocar fallas: `--drop-every 30`, `--latency-ms 500`, `--rate 5`, `--mute`,
`--reject-key`, `--exclusive`, `--occupied`, y `--profile v1` para un foco viejo. La ayuda
completa está al principio de [`examples/foco.rs`](crates/tuya-sim/examples/foco.rs). Para que
se porte como el W509Z2 real, en las pruebas usá `SimConfig::real_firmware(true)` (ver
[protocolo](docs/protocolo.md#lo-aprendido-con-el-w509z2-35)).

Para usarlo desde la app sin una cuenta de Tuya, creá una carpeta **fuera del repo** con este
`devices.json` (son el id y la clave falsos que trae el simulador) e importala desde el primer
paso del asistente:

```json
[
  {
    "id": "openlightsimdevice0001",
    "key": "OpenLightSimKey1",
    "name": "Foco simulado",
    "category": "dj",
    "ip": "127.0.0.1",
    "version": "3.5"
  }
]
```

`tools/golden/contra_sim.py` comprueba que tinytuya real controle al simulador. Ese script y los
generadores de fixtures (`tools/golden/`) usan el entorno del script original, que no es parte
del repo; solo hacen falta si cambiás las fixtures.

## Convenciones

- **Commits chicos**, uno por cambio lógico, que compilen y traigan su prueba. Mensajes **en
  español**, con el área adelante: `tuya-device: …`, `ui: …`, `docs: …`.
- **Español con voseo** en los textos de la app, la documentación y los comentarios.
- Los textos visibles no van en Rust: van como claves i18n en
  `app/ui/src/lib/i18n/{es,en}.json`, en los dos idiomas.
- **`unsafe` está prohibido** (`unsafe_code = "deny"` en el workspace). Las únicas excepciones
  son los módulos de FFI de `crates/capture`; sumar otra hay que justificarlo en el PR.
- El catálogo de ajustes (`openlight-types`) es la única fuente de valores por defecto y rangos.
- Lo portado desde el script en Python lleva la prueba equivalente y su comentario
  `// port: test_ambilight.py::<Clase>::<nombre>`.
- Los detalles del protocolo y de la nube se verifican contra el código de tinytuya, no de
  memoria.

### Nunca datos reales

**El repositorio es público.** No commitees ni pegues en un issue o un PR:

- claves locales, Access ID, Access Secret o tokens;
- ids de foco, IPs, MAC o uid reales;
- `devices.json`, `tuya-raw.json`, `snapshot.json` ni capturas de tu pantalla.

Usá valores inventados. Las respuestas reales de la nube se sanean antes de convertirlas en
fixtures, y se revisa el resultado:

```bash
python3 tools/sanitize.py entrada.json > fixtures/cloud/salida.json
```

Los secretos viajan envueltos en `openlight_types::Secret`, y no se registran en el log.

## Pull requests

1. Trabajá en una rama propia que salga de `main`.
2. Los cambios de contrato (`crates/openlight-types`, `app/src-tauri/src/ipc.rs`,
   `bindings.ts`) van en commits aparte y chicos, idealmente en su propio PR.
3. Abrí el PR contra `main` contando qué cambia, cómo lo probaste y **qué quedó sin probar**
   (sobre todo si no tenés el hardware o el sistema).
4. Se mergea con el **CI en verde** (macOS arm64 y x64, Windows y Linux).

## Releases

Un tag `v*` genera los instaladores de los tres sistemas y deja un **borrador** de Release en
GitHub. Los pasos, las claves del updater y la firma están en
[`docs/release.md`](docs/release.md).

## Licencia

Las contribuciones se publican con la licencia del proyecto: `MIT OR Apache-2.0`.
