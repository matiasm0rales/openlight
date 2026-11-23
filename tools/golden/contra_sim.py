#!/usr/bin/env python3
"""Criterio de aceptación de la tarea B: tinytuya real controla a `tuya-sim`.

Arranca el ejemplo `cargo run -p tuya-sim --example foco` (uno por versión), lo maneja con
tinytuya 1.20 desde el `.venv` del script original y le inyecta fallas por stdin.

    "$HOME/Documents/Open RGB/.venv/bin/python" tools/golden/contra_sim.py [3.1 3.3 3.4 3.5]

Por versión prueba: status, set_value, set_multiple_values, latido, updatedps, reconexión
después de un corte inyectado, "no responde" y, en 3.4/3.5, la clave rechazada (914).
Sale con código 1 si algo falla. No abre nada fuera de 127.0.0.1.
"""

import json
import subprocess
import sys
import time
from pathlib import Path

TINYTUYA = Path.home() / "Documents/Open RGB/.venv/lib/python3.14/site-packages"
sys.path.insert(0, str(TINYTUYA))

import tinytuya  # noqa: E402

RAIZ = Path(__file__).resolve().parents[2]


class Foco:
    """El ejemplo `foco` como subproceso, controlado por stdin/stdout."""

    def __init__(self, version):
        subprocess.run(["cargo", "build", "-q", "-p", "tuya-sim", "--example", "foco"],
                       cwd=RAIZ, check=True)
        binario = RAIZ / "target/debug/examples/foco"
        self.p = subprocess.Popen(
            [str(binario), "--version", version, "--bind", "127.0.0.1:0", "--no-broadcast",
             "--stdin"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, cwd=RAIZ)
        listo = json.loads(self._linea())
        self.port, self.id, self.key = listo["port"], listo["id"], listo["key"]

    def _linea(self):
        linea = self.p.stdout.readline()
        if not linea:
            raise RuntimeError("el simulador terminó")
        return linea.strip()

    def cmd(self, texto):
        self.p.stdin.write(texto + "\n")
        self.p.stdin.flush()
        return json.loads(self._linea())

    def cerrar(self):
        try:
            self.cmd("quit")
        except Exception:
            pass
        self.p.wait(timeout=5)


def dispositivo(foco, version):
    d = tinytuya.Device(foco.id, "127.0.0.1", foco.key, version=float(version), persist=True,
                        connection_timeout=2, connection_retry_limit=2, connection_retry_delay=0.2)
    d.port = foco.port
    return d


def con_reintentos(fn, intentos=4):
    """Como ambilight.py: después de un corte, la primera llamada puede fallar."""
    ultimo = None
    for _ in range(intentos):
        ultimo = fn()
        if ultimo and "Err" not in ultimo:
            return ultimo
        time.sleep(0.2)
    return ultimo


class Resultados:
    def __init__(self):
        self.filas = []

    def check(self, version, nombre, ok, detalle=""):
        self.filas.append((version, nombre, bool(ok), detalle))
        marca = "ok " if ok else "FALLA"
        print(f"  [{marca}] {nombre}" + (f" — {detalle}" if detalle and not ok else ""))
        return ok


def probar(version, r):
    print(f"\n== tinytuya {tinytuya.__version__} contra tuya-sim {version} ==")
    foco = Foco(version)
    try:
        d = dispositivo(foco, version)

        st = d.status()
        r.check(version, "status", st and st.get("dps", {}).get("20") is True, st)

        res = d.set_value(20, False)
        r.check(version, "set_value", res and res.get("dps", {}).get("20") is False
                and foco.cmd("dps")["20"] is False, res)

        res = d.set_multiple_values({"21": "colour", "22": 500, "24": "00f003e803e8"})
        dps = foco.cmd("dps")
        r.check(version, "set_multiple_values",
                res and res.get("dps", {}).get("22") == 500 and dps["22"] == 500
                and dps["24"] == "00f003e803e8", res)

        antes = foco.cmd("stats")["heartbeats"]
        hb = d.heartbeat(nowait=False)
        despues = foco.cmd("stats")["heartbeats"]
        r.check(version, "latido", hb is None and despues == antes + 1, hb)

        res = d.updatedps([20, 22])
        r.check(version, "updatedps", res and res.get("dps", {}).get("22") == 500, res)

        # Corte inyectado: el foco cierra todo; tinytuya tiene que reconectar.
        conexiones = foco.cmd("stats")["connections"]
        foco.cmd("kick")
        time.sleep(0.2)
        st = con_reintentos(d.status)
        r.check(version, "reconexión tras corte",
                st and "dps" in st and foco.cmd("stats")["connections"] > conexiones, st)

        # drop_every: cada conexión dura 0.5 s; los comandos siguen saliendo.
        foco.cmd("drop 0.5")
        time.sleep(0.8)
        res = con_reintentos(lambda: d.set_value(22, 700))
        r.check(version, "reconexión con drop_every",
                res and foco.cmd("dps")["22"] == 700 and foco.cmd("stats")["drops"] >= 1, res)
        foco.cmd("drop off")

        # No responde: tinytuya da error; al volver, anda.
        foco.cmd("mute on")
        d.set_socketRetryLimit(1)
        st = d.status()
        r.check(version, "mudo da error", st is None or "Err" in st, st)
        foco.cmd("mute off")
        d.set_socketRetryLimit(2)
        st = con_reintentos(d.status)
        r.check(version, "se recupera al dejar de estar mudo", st and "dps" in st, st)

        if version in ("3.4", "3.5"):
            foco.cmd("reject on")
            d2 = dispositivo(foco, version)
            st = d2.status()
            r.check(version, "clave rechazada → 914", st and st.get("Err") == "914", st)
            foco.cmd("reject off")
            st = con_reintentos(dispositivo(foco, version).status)
            r.check(version, "con la clave buena vuelve", st and "dps" in st, st)
        d.close()
    finally:
        foco.cerrar()


def main():
    versiones = sys.argv[1:] or ["3.1", "3.3", "3.4", "3.5"]
    r = Resultados()
    for v in versiones:
        try:
            probar(v, r)
        except Exception as e:  # noqa: BLE001
            r.check(v, "sin excepciones", False, repr(e))
    fallas = [f for f in r.filas if not f[2]]
    print(f"\n{len(r.filas) - len(fallas)}/{len(r.filas)} chequeos en verde")
    sys.exit(1 if fallas else 0)


if __name__ == "__main__":
    main()
