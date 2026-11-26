#!/usr/bin/env python3
"""CPU y memoria de OpenLight instalada (macOS), sumando sus procesos de WebKit.

    python3 tools/medir_app.py [--segundos 60] [--nombre openlight]

Solo lee: usa `ps` y `top`, no toca la app. La CPU está en % de un núcleo (como Monitor de
Actividad) y la memoria es la columna `MEM` de `top` (la "Memoria" de Monitor de Actividad).

Los procesos de WebKit (la página, la GPU y la red) los arranca el sistema, no la app: se
reconocen porque macOS la tiene anotada como su responsable. "total" suma la app y la página
(WebContent), que es como se midió el criterio de `docs/PLAN.md` 4.3; "todo" suma además la GPU
y la red.
"""

import argparse
import ctypes
import subprocess
import sys
import time

TIPOS = {
    "com.apple.WebKit.WebContent": "webcontent",
    "com.apple.WebKit.GPU": "gpu",
    "com.apple.WebKit.Networking": "red",
}
UNIDADES = {"B": 1 / 1048576, "K": 1 / 1024, "M": 1, "G": 1024}


def procesos(nombre):
    """{pid: tipo} de la app y de los procesos de WebKit a su cargo."""
    responsable = ctypes.CDLL(None).responsibility_get_pid_responsible_for_pid
    responsable.restype = ctypes.c_int
    responsable.argtypes = [ctypes.c_int]
    salida = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True, check=True).stdout
    filas = [(int(pid), ruta.rsplit("/", 1)[-1]) for pid, ruta in (f.split(None, 1) for f in salida.splitlines())]
    apps = {pid for pid, comm in filas if comm == nombre}
    if len(apps) != 1:
        sys.exit(f"tiene que haber un solo proceso «{nombre}» andando (hay {len(apps)})")
    encontrados = {pid: "app" for pid in apps}
    for pid, comm in filas:
        if comm in TIPOS and responsable(pid) in apps:
            encontrados[pid] = TIPOS[comm]
    return encontrados


def muestra(pid):
    """(segundos de CPU, MB) según `top`, o `None` si el proceso ya no está."""
    salida = subprocess.run(
        ["top", "-l", "1", "-pid", str(pid), "-stats", "pid,time,mem"], capture_output=True, text=True, check=True
    ).stdout
    for fila in reversed(salida.splitlines()):
        campos = fila.split()
        if len(campos) >= 3 and campos[0] == str(pid):
            cpu = 0.0
            for parte in campos[1].rstrip("+-").split(":"):
                cpu = cpu * 60 + float(parte)
            memoria = campos[2].rstrip("+-")
            return cpu, float(memoria[:-1]) * UNIDADES[memoria[-1]]
    return None


def main():
    opciones = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    opciones.add_argument("--segundos", type=float, default=60)
    opciones.add_argument("--nombre", default="openlight", help="nombre del ejecutable")
    args = opciones.parse_args()

    antes = procesos(args.nombre)
    muestras = {pid: [] for pid in antes}
    inicio = time.monotonic()
    while True:
        for pid in muestras:
            if dato := muestra(pid):
                muestras[pid].append((time.monotonic(), *dato))
        if time.monotonic() - inicio >= args.segundos:
            break
        time.sleep(2)

    filas = {}
    for pid, datos in muestras.items():
        if len(datos) < 2:
            continue
        (t0, c0, _), (t1, c1, _) = datos[0], datos[-1]
        filas[antes[pid]] = (100 * (c1 - c0) / (t1 - t0), sum(d[2] for d in datos) / len(datos))
    despues = procesos(args.nombre)
    if despues != antes:
        print("aviso: cambiaron los procesos mientras se medía (¿se abrió o cerró la ventana?)")

    print(f"{'proceso':<12}{'CPU':>8}{'memoria':>12}")
    for nombre in ("app", "webcontent", "gpu", "red"):
        if nombre in filas:
            cpu, memoria = filas[nombre]
            print(f"{nombre:<12}{cpu:>7.1f}%{memoria:>9.0f} MB")
        elif nombre == "webcontent":
            print(f"{nombre:<12}{'sin proceso: la ventana está cerrada':>40}")
    for titulo, nombres in (("total", ("app", "webcontent")), ("todo", tuple(filas))):
        presentes = [filas[n] for n in nombres if n in filas]
        print(f"{titulo:<12}{sum(f[0] for f in presentes):>7.1f}%{sum(f[1] for f in presentes):>9.0f} MB")


if __name__ == "__main__":
    main()
