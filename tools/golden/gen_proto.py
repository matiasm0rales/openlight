#!/usr/bin/env python3
"""Genera fixtures de protocolo (fixtures/proto/*.json) con el código real de tinytuya.

Las tareas A (tuya-proto) y B (tuya-sim) tienen que reproducir estos frames **byte a byte**.

- Corre con el Python del script original, que tiene tinytuya instalado:
      "~/Documents/Open RGB/.venv/bin/python" tools/golden/gen_proto.py
- No abre sockets: usa los métodos de tinytuya que arman y desarman mensajes.
- Para que la salida sea determinista se parchean `os.urandom` (nonces de sesión e IVs de GCM)
  y `time.time` (campo "t" de los payloads). Los valores usados quedan en cada fixture.
- La clave y el id son falsos: nunca se usan datos reales.

Cada fixture `vXY.json` tiene:
  - `negotiation` (3.4 y 3.5): los tres frames del intercambio de clave de sesión, los nonces y la
    clave de sesión resultante.
  - `client_frames`: lo que manda la app al foco (consulta, control, latido...).
  - `device_frames`: lo que manda el foco, y cómo lo decodifica tinytuya (`decoded`).
`udp.json` tiene broadcasts de discovery (6666, 6667, 7000) y el pedido REQ_DEVINFO.
"""

import hashlib
import hmac
import json
import os
import struct
import sys
import time
from hashlib import sha256
from pathlib import Path

TINYTUYA = Path.home() / "Documents/Open RGB/.venv/lib/python3.14/site-packages"
sys.path.insert(0, str(TINYTUYA))

import tinytuya  # noqa: E402
from tinytuya.core import command_types as CT  # noqa: E402
from tinytuya.core import header as H  # noqa: E402
from tinytuya.core.crypto_helper import AESCipher  # noqa: E402
from tinytuya.core.message_helper import TuyaMessage, pack_message, unpack_message  # noqa: E402
from tinytuya.core.udp_helper import decrypt_udp, encrypt as udp_encrypt, udpkey  # noqa: E402

SALIDA = Path(__file__).resolve().parents[2] / "fixtures" / "proto"

DEV_ID = "openlightfakedevice001"  # 22 caracteres, como los ids reales
LOCAL_KEY = "OpenLightKey0001"  # 16 caracteres, falsa
T_FIJO = 1_795_392_000  # 2026-11-23T00:00:00Z
REMOTE_NONCE = bytes(range(0xA0, 0xB0))  # nonce "del foco" en la negociación
DEVICE_IV = bytes(range(0xC0, 0xCC))  # IV de GCM de los frames que arma "el foco" (3.5)

# ---------- determinismo ----------

_contador = {"n": 0}
aleatorios_usados = []


def urandom_fijo(n):
    """Bytes 'aleatorios' reproducibles: sha256(contador) truncado."""
    _contador["n"] += 1
    datos = hashlib.sha256(f"openlight-golden-{_contador['n']}".encode()).digest()[:n]
    aleatorios_usados.append(datos.hex())
    return datos


os.urandom = urandom_fijo
time.time = lambda: float(T_FIJO)


def hx(b):
    return b.hex() if isinstance(b, (bytes, bytearray)) else b


def texto_o_hex(b):
    try:
        return {"utf8": b.decode("utf-8")}
    except UnicodeDecodeError:
        return {"hex": b.hex()}


def nombre_cmd(cmd):
    for k, v in vars(CT).items():
        if k.isupper() and v == cmd:
            return k
    return str(cmd)


# ---------- frames del cliente ----------

def dispositivo(version):
    dev = tinytuya.Device(DEV_ID, "127.0.0.1", LOCAL_KEY, version=version)
    dev.set_socketPersistent(True)
    return dev


def frame_cliente(dev, nombre, payload_msg):
    seq = dev.seqno
    frame = dev._encode_message(payload_msg)
    return {
        "name": nombre,
        "cmd": payload_msg.cmd,
        "cmd_name": nombre_cmd(payload_msg.cmd),
        "seqno": seq,
        "key_hex": dev.local_key.hex(),
        "plaintext": texto_o_hex(payload_msg.payload),
        "frame_hex": frame.hex(),
    }


def negociar(dev):
    """Intercambio de clave de sesión 3.4/3.5 con el foco simulado acá mismo."""
    real = dev.real_local_key
    paso1 = dev._negotiate_session_key_generate_step_1()
    f1 = frame_cliente(dev, "sess_key_neg_start", paso1)
    local_nonce = dev.local_nonce

    # Respuesta del foco (cmd 4): nonce remoto + HMAC(local_key, nonce local)
    resp = REMOTE_NONCE + hmac.new(real, local_nonce, sha256).digest()
    if dev.version == 3.4:
        cifrado = AESCipher(real).encrypt(resp, use_base64=False, pad=True)
        msg = TuyaMessage(1, CT.SESS_KEY_NEG_RESP, 0, struct.pack(">I", 0) + cifrado, 0, True,
                          H.PREFIX_55AA_VALUE, False)
    else:
        msg = TuyaMessage(1, CT.SESS_KEY_NEG_RESP, 0, resp, 0, True, H.PREFIX_6699_VALUE, DEVICE_IV)
    frame2 = pack_message(msg, hmac_key=real)
    desarmado = unpack_message(frame2, hmac_key=real, no_retcode=False)
    assert desarmado.crc_good, "tinytuya no aceptó la respuesta del foco"

    paso3 = dev._negotiate_session_key_generate_step_3(desarmado)
    assert paso3, "tinytuya rechazó el paso 2 de la negociación"
    f3 = frame_cliente(dev, "sess_key_neg_finish", paso3)
    dev._negotiate_session_key_generate_finalize()
    return {
        "local_key_hex": real.hex(),
        "local_nonce_hex": local_nonce.hex(),
        "remote_nonce_hex": REMOTE_NONCE.hex(),
        "device_iv_hex": DEVICE_IV.hex() if dev.version >= 3.5 else None,
        "frames": [
            f1,
            {"name": "sess_key_neg_resp", "cmd": CT.SESS_KEY_NEG_RESP, "cmd_name": "SESS_KEY_NEG_RESP",
             "seqno": 1, "direction": "device_to_client", "key_hex": real.hex(),
             "plaintext": {"hex": resp.hex()}, "frame_hex": frame2.hex()},
            f3,
        ],
        "session_key_hex": dev.local_key.hex(),
    }


# ---------- frames del foco ----------

def frame_del_foco(dev, nombre, cmd, cuerpo, seqno, con_encabezado):
    """Arma un frame como lo mandaría el foco y verifica que tinytuya lo decodifique."""
    key = dev.local_key
    v = dev.version
    plano = (dev.version_header + cuerpo) if con_encabezado else cuerpo
    if not cuerpo and v < 3.5:
        # Respuestas vacías (latido): solo el retcode, sin cifrar
        msg = TuyaMessage(seqno, cmd, 0, struct.pack(">I", 0), 0, True, H.PREFIX_55AA_VALUE, False)
        hmac_key = key if v >= 3.4 else None
        frame = pack_message(msg, hmac_key=hmac_key)
        plano = b""
    elif v >= 3.5:
        msg = TuyaMessage(seqno, cmd, 0, plano, 0, True, H.PREFIX_6699_VALUE, DEVICE_IV)
        frame = pack_message(msg, hmac_key=key)
        hmac_key = key
    elif v >= 3.4:
        cifrado = AESCipher(key).encrypt(plano, use_base64=False, pad=True)
        msg = TuyaMessage(seqno, cmd, 0, struct.pack(">I", 0) + cifrado, 0, True, H.PREFIX_55AA_VALUE, False)
        frame = pack_message(msg, hmac_key=key)
        hmac_key = key
    elif v >= 3.2:
        cifrado = AESCipher(key).encrypt(cuerpo, use_base64=False, pad=True)
        if con_encabezado:
            cifrado = dev.version_header + cifrado
        msg = TuyaMessage(seqno, cmd, 0, struct.pack(">I", 0) + cifrado, 0, True, H.PREFIX_55AA_VALUE, False)
        frame = pack_message(msg)
        hmac_key = None
    else:  # 3.1: las respuestas van en JSON plano
        msg = TuyaMessage(seqno, cmd, 0, struct.pack(">I", 0) + cuerpo, 0, True, H.PREFIX_55AA_VALUE, False)
        frame = pack_message(msg)
        hmac_key = None

    desarmado = unpack_message(frame, hmac_key=hmac_key, no_retcode=False)
    assert desarmado.crc_good, f"{nombre}: CRC/HMAC/tag inválido"
    decodificado = dev._decode_payload(desarmado.payload) if desarmado.payload else None
    return {
        "name": nombre,
        "direction": "device_to_client",
        "cmd": cmd,
        "cmd_name": nombre_cmd(cmd),
        "seqno": seqno,
        "retcode": 0,
        "key_hex": key.hex(),
        "plaintext": texto_o_hex(plano),
        "frame_hex": frame.hex(),
        "decoded": decodificado,
    }


def json_compacto(obj):
    return json.dumps(obj, separators=(",", ":")).encode()


DPS = {"20": True, "21": "colour", "22": 1000, "23": 0, "24": "00f003e803e8"}


def fixture_version(version):
    aleatorios_usados.clear()
    dev = dispositivo(version)
    datos = {
        "version": f"{version:.1f}",
        "dev_id": DEV_ID,
        "local_key": LOCAL_KEY,
        "t": T_FIJO,
        "tinytuya_version": tinytuya.__version__,
    }
    if version >= 3.4:
        datos["negotiation"] = negociar(dev)

    from ambilight_ref import valor_color  # noqa: E402  (mismo formato que el script)

    color_dp, color_valor = valor_color("24", 0.6667, 1.0, 0.5)
    ctrl_dp, ctrl_valor = valor_color("gradual", 0.6667, 1.0, 0.5)
    pedidos = [
        ("dp_query", dev.generate_payload(CT.DP_QUERY)),
        ("control_switch_mode", dev.generate_payload(CT.CONTROL, {"20": True, "21": "colour"})),
        ("control_colour_v2", dev.generate_payload(CT.CONTROL, {color_dp: color_valor})),
        ("control_control_data", dev.generate_payload(CT.CONTROL, {ctrl_dp: ctrl_valor})),
        ("heart_beat", dev.generate_payload(CT.HEART_BEAT)),
        ("update_dps", dev.generate_payload(CT.UPDATEDPS, [18, 19, 20])),
    ]
    datos["client_frames"] = [frame_cliente(dev, n, p) for n, p in pedidos]

    if version >= 3.4:
        estado = json_compacto({"protocol": 4, "t": T_FIJO, "data": {"dps": DPS}})
        query = json_compacto({"dps": DPS})
    else:
        estado = json_compacto({"devId": DEV_ID, "dps": DPS, "t": T_FIJO})
        query = estado
    datos["device_frames"] = [
        frame_del_foco(dev, "dp_query_response", CT.DP_QUERY_NEW if version >= 3.4 else CT.DP_QUERY,
                       query, 2, con_encabezado=False),
        frame_del_foco(dev, "status_push", CT.STATUS, estado, 3, con_encabezado=version >= 3.3),
        frame_del_foco(dev, "heart_beat_response", CT.HEART_BEAT, b"", 4, con_encabezado=False),
    ]
    datos["urandom_used_hex"] = list(aleatorios_usados)
    return datos


# ---------- UDP ----------

def fixture_udp():
    aleatorios_usados.clear()
    info = {"ip": "192.0.2.10", "gwId": DEV_ID, "active": 2, "ablilityCode": 0, "encrypt": True,
            "productKey": "fakeproductkey00", "version": "3.3"}
    casos = []

    plano31 = json_compacto(dict(info, version="3.1", encrypt=False))
    f31 = pack_message(TuyaMessage(0, CT.UDP_NEW, 0, struct.pack(">I", 0) + plano31, 0, True,
                                   H.PREFIX_55AA_VALUE, False))
    casos.append({"name": "broadcast_6666_v31", "port": 6666, "frame_hex": f31.hex(),
                  "decoded": json.loads(decrypt_udp(f31))})

    plano33 = json_compacto(info)
    f33 = pack_message(TuyaMessage(0, CT.UDP_NEW, 0, struct.pack(">I", 0) + udp_encrypt(plano33, udpkey),
                                   0, True, H.PREFIX_55AA_VALUE, False))
    casos.append({"name": "broadcast_6667_v33", "port": 6667, "frame_hex": f33.hex(),
                  "decoded": json.loads(decrypt_udp(f33))})

    plano35 = json_compacto(dict(info, version="3.5"))
    f35 = pack_message(TuyaMessage(0, CT.BOARDCAST_LPV34, None, plano35, 0, True, H.PREFIX_6699_VALUE,
                                   DEVICE_IV), hmac_key=udpkey)
    casos.append({"name": "broadcast_7000_v35", "port": 7000, "frame_hex": f35.hex(),
                  "decoded": json.loads(decrypt_udp(f35))})

    # Pedido que manda la app al puerto 7000 (como scanner.py)
    pedido = json.dumps({"from": "app", "ip": "192.0.2.20"}).encode()
    freq = pack_message(TuyaMessage(0, CT.REQ_DEVINFO, None, pedido, 0, True, H.PREFIX_6699_VALUE, True),
                        hmac_key=udpkey)
    casos.append({"name": "req_devinfo_7000", "port": 7000, "direction": "client_to_broadcast",
                  "plaintext": {"utf8": pedido.decode()}, "frame_hex": freq.hex()})
    return {"udpkey_hex": udpkey.hex(), "udpkey_source": "md5(b'yGAdlopoPVldABfn')",
            "tinytuya_version": tinytuya.__version__, "cases": casos,
            "urandom_used_hex": list(aleatorios_usados)}


def main():
    SALIDA.mkdir(parents=True, exist_ok=True)
    for v in (3.1, 3.3, 3.4, 3.5):
        ruta = SALIDA / f"v{str(v).replace('.', '')}.json"
        ruta.write_text(json.dumps(fixture_version(v), indent=2, ensure_ascii=False) + "\n")
        print("escrito", ruta.relative_to(SALIDA.parents[1]))
    ruta = SALIDA / "udp.json"
    ruta.write_text(json.dumps(fixture_udp(), indent=2, ensure_ascii=False) + "\n")
    print("escrito", ruta.relative_to(SALIDA.parents[1]))


if __name__ == "__main__":
    main()
