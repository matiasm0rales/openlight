#!/usr/bin/env python3
"""Sanea JSON de Tuya (devices.json, respuestas de la nube) antes de convertirlo en fixture.

Reemplaza claves locales, tokens, secretos, uid, MAC, IPs, uuid y números de serie por valores
falsos pero con el mismo formato, de modo que las pruebas sigan siendo realistas.

    python3 tools/sanitize.py entrada.json > fixtures/cloud/salida.json
"""

import hashlib
import json
import re
import sys

SENSIBLES = {
    "key", "local_key", "localKey", "access_token", "refresh_token", "accessToken", "secret",
    "client_secret", "access_secret", "apiSecret", "uid", "owner_id", "ownerId", "uuid", "sn",
    "mac", "ip", "lat", "lon", "time_zone", "email", "username", "mobile", "sign", "nonce",
    "id", "devId", "dev_id", "gwId", "gw_id", "node_id", "local_ip",
}
IP = re.compile(r"\b(\d{1,3}\.){3}\d{1,3}\b")


def falso(valor, campo):
    """Valor falso determinista: el mismo dato real da siempre el mismo reemplazo."""
    h = hashlib.sha256(f"{campo}:{valor}".encode()).hexdigest()
    if isinstance(valor, (int, float)) and not isinstance(valor, bool):
        return 0
    texto = str(valor)
    if campo == "mac":
        return ":".join(h[i:i + 2] for i in range(0, 12, 2))
    if campo == "ip" or IP.fullmatch(texto):
        return f"192.0.2.{int(h[:2], 16) % 254 + 1}"  # rango de documentación (RFC 5737)
    return (h * 4)[: len(texto)] if texto else texto


def sanear(obj, campo=None):
    if isinstance(obj, dict):
        return {k: (falso(v, k) if k in SENSIBLES and not isinstance(v, (dict, list)) else sanear(v, k))
                for k, v in obj.items()}
    if isinstance(obj, list):
        return [sanear(v, campo) for v in obj]
    if isinstance(obj, str):
        return IP.sub(lambda m: falso(m.group(0), "ip"), obj)
    return obj


def main():
    datos = json.load(open(sys.argv[1]) if len(sys.argv) > 1 else sys.stdin)
    json.dump(sanear(datos), sys.stdout, indent=2, ensure_ascii=False)
    print()


if __name__ == "__main__":
    main()
