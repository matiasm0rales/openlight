# Fixtures golden

Generadores que ejecutan el **código de referencia** y guardan entradas y salidas en `fixtures/`.
Los crates de Rust tienen que reproducirlas.

| script | fuente | salida | quién la usa |
|---|---|---|---|
| `gen_color.py` | `~/Documents/Open RGB/ambilight.py` | `fixtures/color/*.json` | D (`color-engine`): tono ±0.5°, s/luz/v ±0.005 |
| `gen_proto.py` | tinytuya 1.20 (`.venv` del script) | `fixtures/proto/v31..v35.json`, `udp.json` | A (`tuya-proto`) byte a byte; B y C |

Se corren con el Python del script original, que ya tiene numpy, mss y tinytuya:

```bash
"$HOME/Documents/Open RGB/.venv/bin/python" tools/golden/gen_color.py
"$HOME/Documents/Open RGB/.venv/bin/python" tools/golden/gen_proto.py
```

Son deterministas (semilla fija; `os.urandom` y `time.time` parcheados en `gen_proto.py`): correrlos
dos veces da los mismos archivos. Si cambian, el cambio tiene que estar justificado en el PR.

Reglas:

- `ambilight_ref.py` importa el script original **en modo lectura**. Nada escribe en
  `~/Documents/Open RGB`.
- `gen_proto.py` usa un id y una clave falsos (`openlightfakedevice001` / `OpenLightKey0001`).
  Nunca uses datos reales en una fixture; para respuestas reales de la nube, pasalas antes por
  `tools/sanitize.py`.

## Formato

- `color_de_pantalla.json`: cada caso trae la imagen en BGRA (`bgra_b64`, `width`×`height`, filas
  sin relleno) y los resultados para `mode` ∈ {dominante, promedio} y varios `tono_previo`.
  `h` es `null` cuando no hay color.
- `proto/vXY.json`: `client_frames` (lo que manda la app, con `plaintext`, `seqno`, `key_hex` y
  `frame_hex`), `device_frames` (lo que manda el foco y cómo lo decodifica tinytuya en `decoded`) y,
  en 3.4/3.5, `negotiation` con los nonces, los tres frames y `session_key_hex`.
  `urandom_used_hex` lista los bytes "aleatorios" que consumió tinytuya, en orden
  (nonce local y, en 3.5, el IV de GCM de cada frame del cliente).
