# Protocolo local de Tuya

Cómo habla OpenLight con los focos por la red local, tal como está implementado en:

| Crate | Qué hace |
|---|---|
| [`tuya-proto`](../crates/tuya-proto/src) | frames, cifrado, payloads, negociación y descifrado de broadcasts. Sin IO ni aleatoriedad: nonces, IV y hora entran por parámetro |
| [`tuya-device`](../crates/tuya-device/src) | la conexión TCP: un actor por foco, con latidos, reconexión y DPs |
| [`tuya-discovery`](../crates/tuya-discovery/src) | la escucha UDP |
| [`tuya-sim`](../crates/tuya-sim/src) | el lado del foco, para probar sin hardware |

**Créditos:** Tuya no publica este protocolo. Todo sale del trabajo de
[tinytuya](https://github.com/jasonacox/tinytuya) (MIT), y se verifica contra la versión 1.20:
las fixtures de `fixtures/proto/` las genera su código y `tuya-proto` las reproduce byte a byte.

Las claves y los ids de este documento son inventados.

## Panorama

El foco escucha en **TCP 6668** y se anuncia por **UDP**. Los números son big-endian.

| Versión | Frame | Cifrado | Integridad | Clave de sesión |
|---|---|---|---|---|
| 3.1 | `55AA` | solo los `CONTROL`: AES-128-ECB + base64 | CRC32 | no |
| 3.3 | `55AA` | AES-128-ECB | CRC32 | no |
| 3.4 | `55AA` | AES-128-ECB | HMAC-SHA256 | sí |
| 3.5 | `6699` | AES-128-GCM | tag GCM | sí |

Claves en juego:

- **local_key**: 16 bytes propios de cada foco (texto interpretado como latin-1). Solo se
  obtiene de la nube de Tuya y cambia al volver a vincular el foco.
- **clave de sesión**: en 3.4 y 3.5, se negocia en cada conexión y reemplaza a la local_key.
- **clave UDP**: la misma para todos, `md5("yGAdlopoPVldABfn")`, solo para los anuncios.

## Frames

### `55AA` (3.1 a 3.4)

```text
prefijo 0x000055AA u32 │ seq u32 │ cmd u32 │ len u32 │ [retcode u32] │ payload │ CRC32 u32 ó HMAC 32 │ sufijo 0x0000AA55 u32

len = [retcode] + payload + CRC/HMAC + sufijo
```

- El **retcode** solo aparece en lo que manda el foco (0 = bien).
- CRC32 en 3.1 y 3.3; HMAC-SHA256 con la clave vigente en 3.4. Los dos cubren desde el prefijo
  hasta el final del payload.

### `6699` (3.5)

```text
prefijo 0x00006699 u32 │ 0 u16 │ seq u32 │ cmd u32 │ len u32 │ IV 12 │ GCM([retcode] + payload) │ tag 16 │ sufijo 0x00009966 u32

len = IV + texto cifrado + tag          (el sufijo no se cuenta)
AAD = encabezado sin el prefijo (14 bytes)
```

El IV es nuevo y aleatorio en cada frame. El retcode, cuando está, va **dentro** del texto
cifrado.

### Lectura del stream

TCP no respeta límites de frames, así que `StreamDecoder` acepta bytes de a pedazos y varios
frames juntos:

- descarta todo hasta el primer prefijo (`55AA` o `6699`); si no hay ninguno, conserva los
  últimos 3 bytes por si quedó un prefijo cortado;
- rechaza un `len` mayor a 1440 (stream corrupto), saltea ese prefijo y sigue;
- un CRC, HMAC o tag inválido descarta ese frame y se continúa con el siguiente.

## Cifrado de los payloads

El **encabezado de versión** son 15 bytes: `"3.x"` seguido de 12 ceros. No lo llevan las
consultas ni los comandos de servicio (`DP_QUERY`, `DP_QUERY_NEW`, `UPDATEDPS`, `HEART_BEAT`, los
tres de negociación y `LAN_EXT_STREAM`).

De la app al foco:

| Versión | Payload |
|---|---|
| 3.1 | JSON en claro. En `CONTROL`: `"3.1"` + firma (16) + base64(ECB(JSON)) |
| 3.3 | ECB(JSON); con encabezado: encabezado **en claro** + ECB(JSON) |
| 3.4 | ECB(\[encabezado] + JSON): el encabezado va cifrado |
| 3.5 | GCM(\[encabezado] + JSON), sin retcode |

La firma de 3.1 son los caracteres 8 a 24 del hexadecimal de
`md5("data=" + base64 + "||lpv=3.1||" + local_key)`. El relleno de ECB es PKCS7.

Del foco a la app es el mismo esquema con el retcode adelante. Para descifrar, como tinytuya:
en 3.4 primero se descifra todo; si lo que queda empieza con el encabezado de la versión, se
quita; en 3.3 recién ahí se descifra. Un payload vacío (por ejemplo, un ack) es válido. Si el
JSON trae los DPs en `data.dps`, se copian a `dps`.

## Negociación de la clave de sesión (3.4 y 3.5)

Es lo primero que pasa en cada conexión. Los tres mensajes van protegidos con la **local_key**.

```text
app  ── cmd 3 ──►  nonce_app (16 bytes aleatorios)
app  ◄── cmd 4 ──  nonce_foco (16) + HMAC-SHA256(local_key, nonce_app) (32)
app  ── cmd 5 ──►  HMAC-SHA256(local_key, nonce_foco)
```

La app verifica el HMAC del paso 2. El foco no contesta el paso 3. Después, los dos calculan:

```text
x = nonce_app XOR nonce_foco
3.4: clave = AES-128-ECB(local_key, x)                           sin relleno
3.5: clave = AES-128-GCM(local_key, iv = nonce_app[0..12], x)    los 16 bytes cifrados, sin el tag
```

Desde ahí todo usa la clave de sesión. El `seq` de la app arranca en 1 en cada conexión y sube
con cada frame.

Igual que tinytuya, **cualquier falla de la negociación** después de conectar el socket se
informa como clave rechazada (el error 914). Eso incluye casos en los que la clave está bien: un
foco que ya tiene otra conexión y cierra la nueva, o uno colgado. Ver
[`device_key_rejected`](errores.md#device_key_rejected).

## Comandos

| cmd | Nombre | Quién | Uso en OpenLight |
|---|---|---|---|
| 3 | `SESS_KEY_NEG_START` | app | negociación, paso 1 |
| 4 | `SESS_KEY_NEG_RESP` | foco | negociación, paso 2 |
| 5 | `SESS_KEY_NEG_FINISH` | app | negociación, paso 3 |
| 7 | `CONTROL` | app | escribir DPs (3.1 y 3.3) |
| 8 | `STATUS` | foco | aviso de que cambiaron DPs, sin que se lo pidan |
| 9 | `HEART_BEAT` | ambos | señal de vida |
| 10 (0x0a) | `DP_QUERY` | app | leer el estado (3.1 y 3.3) |
| 13 (0x0d) | `CONTROL_NEW` | app | escribir DPs (3.4 y 3.5) |
| 16 (0x10) | `DP_QUERY_NEW` | app | leer el estado (3.4 y 3.5) |
| 18 (0x12) | `UPDATEDPS` | app | pedir que refresque DPs; `tuya-proto` lo arma, la app no lo usa |
| 0x13 | `UDP_NEW` | foco | anuncio UDP (6666 y 6667) |
| 0x23 | `BOARDCAST_LPV34` | foco | anuncio UDP (7000) |
| 0x25 | `REQ_DEVINFO` | app | pedir a los focos 3.5 que se anuncien |

En 3.4 y 3.5, `CONTROL` se convierte en `CONTROL_NEW` y `DP_QUERY` en `DP_QUERY_NEW`.

### Payloads

JSON compacto, con las claves en el mismo orden que tinytuya. `t` es la hora en segundos.

| Pedido | 3.1 y 3.3 | 3.4 y 3.5 |
|---|---|---|
| leer | `{"gwId":"…","devId":"…","uid":"…","t":"…"}` | `{}` |
| escribir | `{"devId":"…","uid":"…","t":"…","dps":{"20":true}}` | `{"protocol":5,"t":…,"data":{"dps":{"20":true}}}` |
| latido | `{"gwId":"…","devId":"…"}` | igual |

En 3.1 y 3.3, `gwId`, `devId` y `uid` son el id del foco y `t` es un texto; en 3.4 y 3.5, `t` es
un número.

## DPs y capacidades

Un foco es un conjunto de *data points* numerados. Qué número es cada cosa cambia según el
modelo, así que se toma del **mapeo** que da la nube, buscando por código (los `_v2` ganan):

| Capacidad | Códigos | DP típico (nuevos) | DP típico (viejos) |
|---|---|---|---|
| encendido | `switch_led`, `switch_led_1`, `switch` | 20 | 1 |
| modo (`white`, `colour`, `scene`, `music`) | `work_mode` | 21 | 2 |
| brillo | `bright_value_v2`, `bright_value` | 22 (10 a 1000) | 3 (25 a 255) |
| temperatura | `temp_value_v2`, `temp_value` | 23 (0 a 1000) | 4 (0 a 255) |
| color | `colour_data_v2`, `colour_data` | 24 | 5 |
| control en vivo | `control_data` | 28 | — |
| escena | `scene_data_v2`, `scene_data` | 25 | — |

Sin mapeo (o sin esos códigos) se deduce por los números presentes, como `detect_bulb` de
tinytuya: con el 20 es un foco nuevo; con el 1, uno viejo (si el DP 2 es un número, es solo de
brillo); con los dos a la vez no se considera un foco. Los DPs de control en vivo no aparecen al
leer el estado, así que sin mapeo no se conocen.

### Formatos de color

| Formato | Valor | Ejemplo (tono 124°, sat. 31,6 %, brillo 72,8 %) |
|---|---|---|
| `colour_data_v2` | `hhhh ssss vvvv` en hexa; h 0–360, s y v 0–1000 | `007c013c02d8` |
| `colour_data` | `rrggbb hhhh ss vv`; s y v 0–255 | `7fba83007c51ba` |
| `control_data` | transición (`0` directa, `1` gradual) + hsv de 12 + `00000000` | `0007c013c02d800000000` |

Los redondeos van al par (como `round` de Python) y **el brillo nunca baja de 1 %**: pedir 0
apaga el foco y después tarda en volver. El ajuste *Canal del foco* elige entre `colour_data` y
`control_data`; un foco sin `control_data` usa siempre el color.

## La conexión

Valores por defecto de `tuya-device`; los tres primeros son ajustes.

| Qué | Valor |
|---|---|
| latido | cada 10 s (nunca más de la mitad del siguiente) |
| conexión muerta | tras 30 s sin recibir ningún byte |
| reintento | a los 3 s, después 6 y 12 (tope) |
| conectar, con negociación | 3 s |
| escribir o esperar una respuesta | 2 s |
| espera al arrancar | hasta 45 s, probando cada 1 s |

- Al conectar se lee el estado. Antes de tocar nada se guarda el **estado original** (encendido,
  modo, color, brillo y temperatura) en disco: si la app muere, se recupera en la próxima.
- Para seguir la pantalla se escribe encendido = sí y modo = `colour`, y después los colores
  **sin esperar respuesta**. Si llegan varios antes de poder mandarlos, sale solo el último.
- Se lee continuamente todo lo que manda el foco, para que su búfer no se llene.
- Al reconectar con el efecto activo se repiten encendido y modo (el foco puede haber arrancado
  de otra forma si se cortó la luz) y se reenvía el último color.
- Al restaurar se lee el estado y se escriben **solo los DPs que difieren** del original.

### Cómo se reconoce una respuesta

Cuando sí se espera respuesta, **no se exige que repita nuestro `seq`**:

| Pedido | Cuenta como respuesta |
|---|---|
| leer el estado | un frame de ese comando que traiga DPs |
| escribir | un `STATUS` cuyos DPs incluyan los que se mandaron, con esos valores; o un ack del mismo comando **y** con nuestro `seq` |
| latido | un frame cmd 9 |

Un `STATUS` de un envío anterior, o de un cambio hecho desde el celular, no confirma nada. Si el
control vence sin respuesta, se lee el estado: si el foco ya tiene los valores pedidos, está
cumplido; si no, es un `timeout`.

## Lo aprendido con el W509Z2 (3.5)

El comportamiento del foco real difiere del que simulan las pruebas de tinytuya. Salió de la
prueba con hardware (I2), midiendo con tinytuya contra el foco.

- **Numera sus frames con su propio `seq`.** Usa un contador suyo (en la medición andaba por
  17 500 mientras el de la app iba por 1 a 4). No sirve para emparejar respuestas.
- **A un control contesta con un push `STATUS`** con el estado nuevo. Es lo que confirma el
  pedido. También se observó un ack `CONTROL_NEW` (0x0d) numerado con su contador; como OpenLight
  le exige a un ack su propio `seq`, no lo toma como confirmación. El modo `real_firmware` del
  simulador reproduce el caso más estricto: solo el push, sin ack.
- **El latido lo contesta con cmd 9.**
- Como la confirmación es un cambio de estado, escribir lo que ya tiene puede no generar ningún
  push. Por eso se lee el estado cuando un control no recibe respuesta, y al restaurar se escribe
  solo lo que cambió.
- Acepta **una sola conexión** a la vez.

Antes de estos arreglos
([#17](https://github.com/matiasm0rales/openlight/pull/17)), la prueba de colores funcionaba pero
restaurar daba `device_timeout` aunque el foco hubiera obedecido. En las pruebas:

```rust
tuya_sim::start(SimConfig::new(ProtocolVersion::V35).ephemeral().real_firmware(true))
```

## Discovery (UDP)

| Puerto | Quiénes | Frame y cifrado |
|---|---|---|
| 6666 | 3.1 | `55AA`, JSON en claro |
| 6667 | 3.3 y 3.4 | `55AA`, ECB con la clave UDP |
| 7000 | 3.5 | `6699`, GCM con la clave UDP |

OpenLight escucha los tres y además manda cada 6 s un `REQ_DEVINFO`
(`{"from": "app", "ip": "<su ip>"}`) al broadcast de cada interfaz, puerto 7000, que es lo que
hace que los focos 3.5 se anuncien.

De cada anuncio se toma el id (`gwId`), la versión (`version`), el `productKey` y, como IP, la
**dirección de origen del paquete**. Se ignoran los pedidos de otras apps, los datagramas sin
`gwId` y las versiones que OpenLight no habla (por ejemplo 3.2).

La nube da claves y mapeos; la red da IP y versión. Se cruzan por id.

## Por qué la app adopta la versión que anuncia el foco

La versión que quedó guardada puede estar mal. Pasó en la prueba real: al volver a agregar un
foco 3.5 con la búsqueda fallando, quedó registrado como 3.3 y sin IP. Hablarle 3.3 a un foco 3.5
no da un error claro, sino silencio o respuestas que no se descifran, y la app reintentaba así
para siempre.

Ahora:

- **al agregar** un foco, la versión y la IP salen de la red o de lo ya guardado; si no hay
  ninguna, es un error. Nunca se supone 3.3
  ([#18](https://github.com/matiasm0rales/openlight/pull/18)). La única excepción es la
  importación del script, que asume 3.3 **y lo avisa**;
- **con IP automática**, cada vez que se busca el foco (al arrancar, al reconectar, ante un 914 o
  después de un fallo) se usa la versión que anuncia, igual que `auto_ip` de tinytuya. Se guarda
  junto con la IP en `state.json` y se usa en la próxima ejecución
  ([#19](https://github.com/matiasm0rales/openlight/pull/19));
- **con IP fija** no se busca nada: vale lo configurado.

Con IP automática, un 914 en el arranque dispara primero una búsqueda: la IP guardada puede ser
ahora de otro aparato. Recién después se culpa a la clave.

## Lo que no está soportado

- `device22` (focos 3.3 o 3.4 que contestan `data unvalid` a la consulta): `tuya-proto` sabe
  armar sus pedidos, pero la app lo informa como error.
- Subdispositivos Zigbee y gateways.
- Protocolo 3.2.
