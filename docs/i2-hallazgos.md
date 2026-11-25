# I2: hallazgos de la prueba con hardware real (macOS)

La prueba I2 del [plan](PLAN.md) se hace usando la app en el día a día con dos focos reales: una
A60 (protocolo 3.3) y un W509Z2 (3.5). Estado de cada punto al 2026-11-24.

| # | Hallazgo | Estado |
|---|---|---|
| 1 | La app no escribía logs | **hecho** en [#20](https://github.com/matiasm0rales/openlight/pull/20) |
| 2 | W509Z2: `device_timeout` al restaurar, y re-agregado como 3.3 | **arreglado** en [#17](https://github.com/matiasm0rales/openlight/pull/17), [#18](https://github.com/matiasm0rales/openlight/pull/18) y [#19](https://github.com/matiasm0rales/openlight/pull/19) |
| 3 | El llavero de macOS pide la contraseña muchas veces | **arreglado**; falta verificarlo en la app instalada |

## 1. Logger

**Hallazgo:** la app no escribía logs (no había subscriber de `tracing`).
**Pedido:** agregar un logger a archivo con rotación, `RUST_LOG`, sin secretos; alimentar
`diagnostics_log` y `diagnostics_export`.

**Estado: hecho** ([#20](https://github.com/matiasm0rales/openlight/pull/20)). Archivo con
rotación diaria (7 días), `RUST_LOG`, visor en Diagnóstico y exportación a un `.zip` sin
secretos, con una prueba que busca las claves en el registro y en el zip.

## 2. W509Z2 (3.5)

**Hallazgo:** `device_timeout` al restaurar después de la prueba RGB. Confirmado con tinytuya: el
foco numera con su propio contador y a un set con espera responde con un push de `STATUS`
(cmd 8), no con un ack con nuestro `seqno`. Además, un re-agregado con el discovery fallando
guardó el foco como 3.3 sin IP.

**Estado: arreglado.**

- [#17](https://github.com/matiasm0rales/openlight/pull/17): la respuesta a un control es el push
  con los DPs pedidos, sin exigir el mismo `seqno`; si no llega, se lee el estado; al restaurar
  solo se escribe lo que cambió. `tuya-sim` ganó el modo `real_firmware`.
- [#18](https://github.com/matiasm0rales/openlight/pull/18): al agregar un foco nunca se adivina
  la versión ni la IP; si no se lo encuentra, hay un error claro (`device_not_found_on_lan` o
  `lan_scan_failed`).
- [#19](https://github.com/matiasm0rales/openlight/pull/19): la app adopta la versión que anuncia
  el foco, así un registro equivocado se corrige solo.

Los detalles están en [protocolo](protocolo.md#lo-aprendido-con-el-w509z2-35).

## 3. Llavero de macOS

**Hallazgo:** cada recompilación cambia la firma del binario y macOS vuelve a pedir la contraseña
por cada entrada, y había una por secreto (`device:<id>:local_key`, `cloud:access_id` y
`cloud:access_secret`): con la nube y dos focos se pidió tres veces o más. **Pedido:** verificar en
la app instalada que pida una sola vez; evaluar guardar todas las claves en una sola entrada del
llavero.

**Estado: arreglado; falta verificarlo en la app instalada.**

- En macOS todos los secretos van en **una sola entrada** (servicio `openlight`, cuenta `secrets`):
  un JSON que fuera del llavero solo circula envuelto en `Secret`. Se lee una vez por arranque y
  queda en memoria.
- **Migración:** la primera vez que se pide un secreto que la entrada nueva no conoce, se lee la
  entrada anterior, se la pasa a la nueva y se la borra. El resultado queda anotado aunque no
  hubiera nada, así no se busca de nuevo. Por eso **la primera vez después de actualizar, macOS
  todavía pide permiso por cada entrada anterior**; desde la siguiente versión, una sola vez.
- Si la entrada no se puede leer (permiso denegado, llavero bloqueado) o su contenido no se
  entiende, no se escribe nada encima.
- Windows y Linux siguen con una entrada por secreto: una credencial de Windows admite 2560 bytes
  en UTF-16 (menos de 20 focos si fueran todos juntos) y ninguno de los dos pide permiso por
  entrada.
- Volver a una versión anterior después de migrar no encuentra las claves: hay que renovarlas y
  volver a cargar las credenciales de la nube.

Las pruebas usan un llavero falso que cuenta los accesos (`crates/secrets`); **falta** comprobar en
la app instalada cuántas veces pide la contraseña, también al renovar una clave (escribe en la
entrada). Mientras la app no esté firmada con una identidad estable, cada versión nueva puede
volver a pedirla: elegí **Permitir siempre**. Para desarrollar, usá `OPENLIGHT_SECRETS=memory`
(ver [CONTRIBUTING](../CONTRIBUTING.md#preparar-el-entorno)).
