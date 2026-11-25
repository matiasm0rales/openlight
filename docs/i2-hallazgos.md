# I2: hallazgos de la prueba con hardware real (macOS)

La prueba I2 del [plan](PLAN.md) se hace usando la app en el día a día con dos focos reales: una
A60 (protocolo 3.3) y un W509Z2 (3.5). Estado de cada punto al 2026-11-24.

| # | Hallazgo | Estado |
|---|---|---|
| 1 | La app no escribía logs | **hecho** en [#20](https://github.com/matiasm0rales/openlight/pull/20) |
| 2 | W509Z2: `device_timeout` al restaurar, y re-agregado como 3.3 | **arreglado** en [#17](https://github.com/matiasm0rales/openlight/pull/17), [#18](https://github.com/matiasm0rales/openlight/pull/18) y [#19](https://github.com/matiasm0rales/openlight/pull/19) |
| 3 | El llavero de macOS pide la contraseña muchas veces | **pendiente** |

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
por cada entrada. **Pedido:** verificar en la app instalada que pida una sola vez; evaluar
guardar todas las claves en una sola entrada del llavero.

**Estado: pendiente.** Hoy hay una entrada por secreto (`device:<id>:local_key`,
`cloud:access_id` y `cloud:access_secret`, en el servicio `openlight`). Falta la verificación en
la app instalada. Tené presente que, mientras la app no esté firmada con una identidad estable,
cada versión nueva puede volver a disparar el pedido.

Mientras tanto: en cada pedido elegí **Permitir siempre**, y para desarrollar usá
`OPENLIGHT_SECRETS=memory` (ver [CONTRIBUTING](../CONTRIBUTING.md#preparar-el-entorno)).
