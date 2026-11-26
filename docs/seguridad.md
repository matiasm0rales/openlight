# Seguridad

Revisión de seguridad previa a la primera versión pública (tarea Q4 de [`PLAN.md`](PLAN.md)),
hecha el 25 de noviembre de 2026 sobre `main` (hasta el PR #31).

## Qué se protege y de quién

| qué | dónde vive |
|---|---|
| clave local de cada foco | llavero del sistema |
| Access ID y Access Secret de Tuya | llavero del sistema |
| token y firma (`sign`) de la API de Tuya | solo en memoria, mientras dura el pedido |

Los casos que se tuvieron en cuenta: alguien que lee un diagnóstico exportado o un log que la
persona adjuntó a un issue; otro equipo en la misma red; un atacante entre la app e internet; una
actualización falsa; y código que lograra ejecutarse dentro de una ventana de la app. Queda
afuera quien ya controla la sesión del usuario en la computadora (puede leer la memoria del
proceso o pedirle los secretos al llavero).

## Resumen

No apareció ninguna fuga en el uso normal. Sí dos caminos de error que podían mostrar secretos, y
permisos de la interfaz bastante más amplios que lo necesario. Todo eso quedó arreglado y con una
prueba que falla si vuelve.

| # | hallazgo | gravedad | estado |
|---|---|---|---|
| 1 | Si la nube de Tuya contestaba un error repitiendo lo que recibió, el token, la firma y el Access ID llegaban al log, al zip de diagnóstico y a la pantalla | media | arreglado |
| 2 | Importar un `devices.json` o `config.json` mal formado podía mostrar una clave local en el mensaje de error | media | arreglado |
| 3 | Las dos ventanas podían invocar todos los comandos de la app; la principal tenía además menús, tray, imágenes y rutas, y podía abrir cualquier página web | media (defensa en profundidad) | arreglado |
| 4 | La CSP permitía estilos inline y un protocolo sin uso, y no cerraba `object-src`, `base-uri` ni `form-action` | baja | arreglado |
| 5 | El cliente de la nube seguía redirecciones (se habrían llevado la firma y el token) y no exigía HTTPS en el código | baja | arreglado |
| 6 | El `Debug` de la sesión del protocolo y de la conexión imprimía la clave local y la de sesión (nadie lo escribía en el log) | baja | arreglado |
| 7 | El tapado de `config.json` y `devices.json` en el zip no cubría `access_id`, `client_id` ni `sign` (esos archivos no los tienen) | baja | arreglado |
| 8 | El `Debug` de las credenciales de la nube mostraba el Access ID | baja | arreglado en el PR #31 (contrato) |
| 9 | `cookie` 0.18.2 ([RUSTSEC-2026-0594](https://rustsec.org/advisories/RUSTSEC-2026-0594)); la app no usa lo afectado | baja | actualizado a 0.18.3 |

El hallazgo 1 se reprodujo con una nube falsa. No se vio que la API real de Tuya repita esos
valores.

## 1. Secretos

**Revisado:** cada `tracing::*!` de todos los crates, los errores que cruzan el IPC, los eventos,
el zip de "Exportar diagnóstico", `config.json`, `devices.json`, `state.json`, las calibraciones,
el progreso del asistente y lo que la interfaz guarda en `localStorage` (idioma, tema, nivel de
los ajustes y búsqueda de actualizaciones: ningún secreto).

**Cómo se comprueba:** [`tests/secretos.rs`](../app/src-tauri/tests/secretos.rs) recorre el flujo
con el backend real, el **cliente real de la nube** contra una OpenAPI falsa y un foco `tuya-sim`
con una clave falsa: credenciales mal puestas, agregar, probar, sesión, calibración, clave vieja
(914), renovar, importar y una nube que repite lo que recibe. Con el log en `trace` (la app usa
`info` y `debug` solo en los crates propios), busca cada secreto en claro, en hexadecimal, en
base64 y como arreglo de bytes en el log, el zip, la carpeta de datos, las respuestas de los
comandos y los eventos. Usa el llavero en memoria: nunca toca el real.

Lo que sostiene esto en el código:

- `openlight_types::Secret` no se puede serializar y su `Debug` no muestra el valor.
- El cliente de la nube tapa en los mensajes de error el Access ID, el Access Secret, el token y
  la firma, y quita la URL de los errores de red.
- Los errores de importación dicen la línea y la columna, nunca el contenido.
- En macOS todos los secretos van en una sola entrada del llavero (PR #29); en Windows y Linux,
  una por secreto. Los errores del llavero nombran la entrada, no su valor.

### Respaldo para llevar los focos a otra compu

Agregado después de la revisión, con la misma regla: **ningún secreto sale del llavero**.

- El tipo del archivo (`openlight_core::backup::Backup`) no tiene campos para claves, Access ID,
  Access Secret ni IPs, así que no se pueden serializar. Tampoco lleva el mapeo de DPs, que tiene
  un campo de JSON libre: se vuelve a bajar junto con la clave. Una prueba enumera todos los
  campos del archivo y falla si aparece uno nuevo.
- [`tests/respaldo.rs`](../app/src-tauri/tests/respaldo.rs) guarda claves, credenciales e IPs
  falsas, exporta y falla si alguna aparece (en claro, hexadecimal, base64 o como bytes, y
  cualquier cosa con forma de IPv4).
- Un archivo importado se valida entero antes de escribir (versión, ids, zonas, rangos). Los
  errores dicen la línea y la columna, nunca el contenido.
- El archivo se elige con diálogos del sistema. La ventana no puede pedirle al backend que lea o
  escriba una ruta, y ninguna capability le da los comandos de `tauri-plugin-dialog`.
- El respaldo sí tiene nombres e ids de los focos y de los perfiles.

El zip **no tiene secretos, pero sí datos personales**: nombres e ids de los focos, sus IP de la
red local y rutas de archivos (que incluyen el nombre de usuario). Conviene mirarlo antes de
publicarlo, como ya pide [`CONTRIBUTING.md`](../CONTRIBUTING.md).

## 2. Permisos de cada ventana (capabilities)

`build.rs` declara los comandos de la app, así Tauri exige un permiso para cada uno.

| ventana | plugins | comandos de la app |
|---|---|---|
| principal | escuchar eventos, leer la versión, cerrar recursos, abrir `iot.tuya.com` y las releases del repo, inicio automático, updater, reiniciar | todos |
| calibración | escuchar eventos y abrir `iot.tuya.com` | los 17 que usan esa pantalla y las acciones de sus errores (entre ellas, sacar un foco del efecto); no puede cargar credenciales, listar la nube, agregar o quitar focos, ver el diagnóstico, importar ni actualizar (renovar una clave sí consulta la nube) |

[`tests/permisos.rs`](../app/src-tauri/tests/permisos.rs) prueba **todos** los comandos de la app y
de los plugins contra las capabilities reales: un permiso de más o de menos hace fallar la prueba.
También falla si un comando de `ipc.rs` no está registrado o no figura en `build.rs`, y si las
pantallas de calibración llaman a uno que su ventana no puede. Lo que esa ventana usa a través de
las acciones de un error se revisa a mano.

Al agregar un comando hay que sumarlo en `build.rs` y en `capabilities/default.json` (y en
`calibration.json` si lo usa la calibración). Al agregar en la guía un enlace a otro dominio hay
que permitirlo en las dos capabilities.

## 3. Content Security Policy

```
default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self';
connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none';
frame-ancestors 'none'
```

`data:` queda solo para imágenes (vistas previas de los monitores e ilustraciones chicas). Las
fuentes van como archivos (`vite.config.ts`). Los estilos que Svelte pone con `style:` pasan por
el CSSOM, que `style-src 'self'` permite.

[`e2e/csp.spec.ts`](../app/ui/e2e/csp.spec.ts) compila la interfaz como para producción, la sirve
con la política de `tauri.conf.json`, recorre las pantallas y el asistente y falla ante cualquier
bloqueo. `pnpm dev` y `tauri dev` no la aplican.

Además se probó la app compilada en macOS 26 (WebKit y protocolo `tauri://`, con las ventanas
ocultas, una sonda temporal, llavero en memoria y carpeta temporal): panel, ajustes,
dispositivos, diagnóstico, control manual, el último paso del asistente y calibración cargaron sin
ningún bloqueo; un script y un estilo inline inyectados a propósito no corrieron, y la ventana de
calibración recibió "not allowed" al pedir la nube, el diagnóstico y el updater. En Windows y
Linux no se probó así (va con I3).

## 4. Dependencias

`cargo audit`, `pnpm audit` (`app/ui`) y `npm audit` (`web`) corren en
[`dependencias.yml`](../.github/workflows/dependencias.yml) cuando cambia un lockfile y una vez
por semana. Al 25/11/2026: **ninguna vulnerabilidad** en las tres, y estos avisos informativos de
Rust, sin versión compatible que los resuelva:

| crate | aviso | llega por | por qué se acepta |
|---|---|---|---|
| `glib` 0.18 | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429) (*unsound*) | GTK 3 de Tauri, solo Linux | el fallo está en `VariantStrIter`, que no usan ni OpenLight ni las bibliotecas de la interfaz (buscado en su código); se va cuando Tauri deje GTK 3 |
| `proc-macro-error` | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370) | macros de GTK 3, solo Linux | solo en compilación |
| `paste` | [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436) | `specta` y `tauri-specta` (versiones fijadas) | solo en compilación |
| `zbus_names`, `zvariant`, `zvariant_derive` | RUSTSEC-2026-0370 a 0372 | `zbus` 5: llavero, `opener` e instancia única | pasaron a ser módulos de `zbus` 6; hay que esperar a que migren esas dependencias |
| `chrono` | [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) | dependencia opcional de `serde_with`, sin activar | está en `Cargo.lock` pero no se compila |

## 5. Actualizaciones

- `tauri.conf.json` tiene `pubkey`, `requireSignedVersion` y un único endpoint HTTPS de las
  releases del repo. No hay `dangerousInsecureTransportProtocol`, certificados inválidos ni
  vuelta a versiones anteriores.
- Cambiar el endpoint por variable de entorno solo existe en builds de desarrollo.
- [`tests/updater_firma.rs`](../app/src-tauri/tests/updater_firma.rs) corre el plugin real contra
  un `latest.json` local: acepta un paquete bien firmado y **rechaza** uno cambiado, firmado con
  otra clave, sin firma válida, sin versión en la firma o firmado para otra versión.
- Con la clave de ejemplo que hay hoy no se instala nada (falla cerrado).

## 6. Red

- **Nube:** solo HTTPS, certificados verificados (rustls) y sin redirecciones. La única excepción
  a HTTPS es la propia computadora, para las pruebas. Solo se usa al configurar y al renovar claves.
- **Focos:** conexiones TCP salientes al puerto 6668. La app no abre ningún puerto TCP.
- **Búsqueda:** escucha UDP en 6666, 6667 y 7000, en todas las interfaces porque son anuncios por
  *broadcast*, y solo mientras busca. Manda el pedido de anuncio al *broadcast* de cada red.
- `tuya-sim` (que sí escucha TCP) es solo una dependencia de pruebas: no viaja en la app.
- La web de descarga tiene CSP propia, solo inserta HTML con textos propios y solo enlaza
  descargas `https://`.

## Pendiente

### Antes de la primera versión

1. **Generar la clave del updater** y pegar la pública en `tauri.conf.json`
   ([`release.md`](release.md)). Una versión publicada sin ella no se actualiza nunca.
2. **Activar en GitHub** (el repo es público y hoy están apagados): *secret scanning*, *push
   protection* y alertas de Dependabot.

### Mejoras

3. **Release:** fijar las acciones de terceros por SHA (`tauri-action`, `rust-cache`,
   `rust-toolchain`, `install-action`). La clave privada del updater pasa a `GITHUB_ENV` en
   cualquier corrida que tenga los secrets, también la de un PR de una rama del propio repo:
   conviene exportarla solo con un tag.
4. **Anuncios de la red falsos.** El protocolo de Tuya no autentica los anuncios (la clave de ese
   cifrado es pública). Otro equipo de la red puede anunciar otra IP u otra versión para un foco
   y, mientras el real no responda, la app puede quedarse hablándole al falso (incluso en 3.1, la
   versión más débil). No consigue la clave local, pero sí dejar el foco sin respuesta. Se puede
   endurecer aceptando solo versiones más nuevas que la guardada, o pidiendo IP fija.
5. Los secretos no se borran de la memoria al soltarlos (`zeroize`).
6. `freezePrototype` de Tauri no está activado.
7. Los instaladores van sin certificado de Apple ni de Windows ([`release.md`](release.md)).

## Reportar un problema

Si encontrás un problema de seguridad, no pegues claves, `devices.json` ni capturas del portal de
Tuya en un issue público.
