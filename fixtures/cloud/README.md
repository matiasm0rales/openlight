# Fixtures de la Tuya OpenAPI (tarea C)

Las usa `crates/tuya-cloud/tests/cloud.rs` con `wiremock`. **Ninguna sale de una cuenta real**:
se escribieron a mano con la forma que documentan Tuya y tinytuya, y después se pasaron por
`tools/sanitize.py` (por eso los ids, uid, tokens y claves parecen hashes).

| archivo | qué es | de dónde sale la forma |
|---|---|---|
| `firmas.json` | firmas HMAC de 5 pedidos con credenciales falsas y reloj fijo | **generado corriendo tinytuya 1.20** (`generar_firmas.py`) |
| `token_ok.json`, `token_ok_2.json` | `GET /v1.0/token?grant_type=1` | `Cloud.py::_gettoken` (`result.access_token`, `t`) + doc "Get a token" (`expire_time`, `uid`) |
| `devices_page1.json`, `devices_page2.json`, `devices_empty.json` | `/v1.0/iot-01/associated-users/devices`, paginado con `has_more` y `last_row_key` | `Cloud.py::_get_all_devices` (`result.devices`, `has_more`, `last_row_key`) y los campos de `DEVICEFILE_SAVE_VALUES` |
| `devices_by_user.json` | `/v1.3/iot-03/devices?source_type=tuyaUser` (segunda pasada) | `Cloud.py::_get_all_devices(uid)` (`result.list`) |
| `specifications_light.json` | `/v1.1/devices/{id}/specifications` de una luz con DPs 20–28 | `Cloud.py::getdps` + `_build_mapping` (`status`, `functions`, `dp_id`, `values` como texto) |
| `specifications_not_supported.json` | código 2009 | `Cloud.py::getmapping` (`code == 2009` / `'not support this device'`) |
| `error_<código>.json` | respuestas con `success: false` | códigos y mensajes de la página oficial "Global Error Codes" (ver la tabla en `crates/tuya-cloud/src/error.rs`) |

## Regenerar

```bash
# Firmas (no usa la red: intercepta requests y el reloj)
~/Documents/Open\ RGB/.venv/bin/python -I fixtures/cloud/generar_firmas.py > fixtures/cloud/firmas.json

# Cualquier respuesta nueva, real o armada a mano: siempre pasa por el saneador
python3 -I tools/sanitize.py respuesta.json > fixtures/cloud/respuesta.json
```

Si alguna vez se captura una respuesta real (por ejemplo con `cargo run -p tuya-cloud --example
listar`), **se sanea antes de commitearla** y, si contradice algo de acá, manda la real.
