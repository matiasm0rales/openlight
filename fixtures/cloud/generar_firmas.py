#!/usr/bin/env python3
"""Genera `firmas.json`: los encabezados que arma `tinytuya.Cloud._tuyaplatform` con credenciales
falsas y un reloj fijo, para que `tuya-cloud` compare su firma byte a byte.

No toca la red: se reemplazan `requests.get`/`requests.request` por funciones que guardan el
pedido y devuelven una respuesta armada a mano.

    ~/Documents/Open\\ RGB/.venv/bin/python fixtures/cloud/generar_firmas.py > fixtures/cloud/firmas.json
"""

import json
import sys
import time

import tinytuya
import tinytuya.Cloud  # noqa: F401  (registra el módulo)
cloud_mod = sys.modules["tinytuya.Cloud"]

ACCESS_ID = "fakeaccessid00000000"
ACCESS_SECRET = "fakeaccesssecret0000000000000000"
TOKEN = "faketoken0000000000000000000000"
NOW_MS = 1700000000123

capturados = []


class Respuesta:
    def __init__(self, cuerpo):
        self.text = json.dumps(cuerpo)
        self.content = self.text.encode()
        self.status_code = 200


def respuesta_para(url):
    if "/token" in url:
        return {"success": True, "t": NOW_MS, "result": {"access_token": TOKEN, "expire_time": 7200,
                                                         "refresh_token": "fakerefresh", "uid": "fakeuid"}}
    return {"success": True, "t": NOW_MS, "result": {}}


def falso_get(url, headers=None, **kw):
    capturados.append({"method": "GET", "url": url, "headers": dict(headers)})
    return Respuesta(respuesta_para(url))


def falso_request(method, url, headers=None, data=None, **kw):
    capturados.append({"method": method, "url": url, "headers": dict(headers), "body": data})
    return Respuesta(respuesta_para(url))


cloud_mod.requests.get = falso_get
cloud_mod.requests.request = falso_request
cloud_mod.time.time = lambda: NOW_MS / 1000.0

casos = []


def caso(nombre, fn):
    capturados.clear()
    fn()
    pedido = capturados[-1]
    h = pedido["headers"]
    casos.append({
        "name": nombre,
        "method": pedido["method"],
        "url": pedido["url"],
        "token": h.get("access_token"),
        "t": h["t"],
        "sign": h["sign"],
    })


c = tinytuya.Cloud(apiRegion="us", apiKey=ACCESS_ID, apiSecret=ACCESS_SECRET, initial_token=None)
# El constructor ya pidió el token: ese es el primer caso.
casos.append({
    "name": "token",
    "method": capturados[0]["method"],
    "url": capturados[0]["url"],
    "token": None,
    "t": capturados[0]["headers"]["t"],
    "sign": capturados[0]["headers"]["sign"],
})
caso("devices_first_page", lambda: c.cloudrequest("/v1.0/iot-01/associated-users/devices", query={"size": "50"}))
caso("devices_next_page", lambda: c.cloudrequest("/v1.0/iot-01/associated-users/devices",
                                                  query={"size": "50", "last_row_key": "FAKEROWKEY01"}))
caso("devices_by_user", lambda: c.cloudrequest("/v1.3/iot-03/devices",
                                                query={"page_size": "75", "source_type": "tuyaUser", "source_id": "fakeuid"}))
caso("specifications", lambda: c.getdps("openlightfakedevice001"))

json.dump({
    "source": "tinytuya %s, Cloud._tuyaplatform (new_sign_algorithm=True)" % tinytuya.__version__,
    "access_id": ACCESS_ID,
    "access_secret": ACCESS_SECRET,
    "region": "us",
    "cases": casos,
}, sys.stdout, indent=2)
print()
