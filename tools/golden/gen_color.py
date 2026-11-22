#!/usr/bin/env python3
"""Genera fixtures de color (fixtures/color/*.json) ejecutando el `ambilight.py` original.

La tarea D (color-engine) tiene que reproducir estas salidas con tolerancia ≤0.5° de tono y
≤0.005 en saturación, luz y brillo.

    "~/Documents/Open RGB/.venv/bin/python" tools/golden/gen_color.py

Todo es determinista (numpy con semilla fija). Las imágenes van en BGRA (como las entrega la
captura), codificadas en base64, fila por fila sin relleno.
"""

import base64
import json
import random
from pathlib import Path

import numpy as np

from ambilight_ref import (
    CONSTANTES,
    Calibracion,
    Suavizador,
    color_de_pantalla,
    diferencia,
    elegir_monitor,
    elegir_original,
    objetivo_del_foco,
    separacion_tonos,
    valor_color,
)

SALIDA = Path(__file__).resolve().parents[2] / "fixtures" / "color"
SEMILLA = 20261122

# Colores de test_ambilight.py (RGB)
AZUL = (30, 80, 220)
NARANJA = (240, 130, 20)
ROJO = (220, 30, 30)
VERDE = (30, 200, 60)
ROJO_PURO = (230, 40, 30)
AMBAR = (200, 120, 40)
BLANCO = (245, 245, 245)
NEGRO = (0, 0, 0)

# La calibración real del usuario (no es secreta): una tabla con correcciones fuertes
CALIBRACION_REAL = {
    0: {"tono": 0, "sat": 1.0}, 30: {"tono": 26, "sat": 1.3}, 60: {"tono": 54, "sat": 1.3},
    90: {"tono": 100, "sat": 1.0}, 120: {"tono": 100, "sat": 1.0}, 150: {"tono": 92, "sat": 1.0},
    180: {"tono": 138, "sat": 0.6}, 210: {"tono": 159, "sat": 1.0}, 240: {"tono": 240, "sat": 1.0},
    270: {"tono": 290, "sat": 1.0}, 300: {"tono": 346, "sat": 1.0}, 330: {"tono": 357, "sat": 1.0},
}
# Tabla con tonos negativos y > 360, como las que puede dejar calibrar.py
CALIBRACION_BORDES = {0: {"tono": -10, "sat": 0.8}, 120: {"tono": 130, "sat": 1.5},
                      240: {"tono": 250, "sat": 1.0}, 330: {"tono": 365, "sat": 0.4}}


def imagen(color=NEGRO, alto=36, ancho=64):
    img = np.zeros((alto, ancho, 4), np.uint8)
    return pintar(img, color)


def pintar(img, color, filas=slice(None), columnas=slice(None)):
    img[filas, columnas, 0] = color[2]
    img[filas, columnas, 1] = color[1]
    img[filas, columnas, 2] = color[0]
    return img


def b64(img):
    return base64.b64encode(np.ascontiguousarray(img).tobytes()).decode()


def f(x):
    return None if x is None else float(x)


# ---------- escenas ----------

def escenas_de_las_pruebas():
    """Las imágenes de la clase ColorDePantalla de test_ambilight.py."""
    e = {}
    e["opuestos_azul_naranja"] = pintar(imagen(AZUL), NARANJA, columnas=slice(32, None))
    e["rojo_puro_y_verde"] = pintar(imagen(ROJO_PURO), VERDE, columnas=slice(32, None))
    blanca = imagen(BLANCO)
    e["pagina_blanca_foto_chica"] = pintar(blanca, ROJO, slice(14, 22), slice(26, 38))
    e["pastel"] = imagen((230, 220, 200))
    e["negro"] = imagen(NEGRO)
    e["gris"] = imagen((120, 120, 120))
    e["gris_casi_puro"] = imagen((200, 200, 202))
    bandas = imagen(AMBAR)
    pintar(bandas, (10, 10, 10), slice(0, 6))
    e["bandas_negras"] = pintar(bandas, (10, 10, 10), slice(30, 36))
    laterales = imagen(AMBAR)
    pintar(laterales, NEGRO, columnas=slice(0, 8))
    e["bandas_laterales"] = pintar(laterales, NEGRO, columnas=slice(56, 64))
    e["rojo_y_azul_parecidos"] = pintar(imagen(ROJO), (30, 30, 220), columnas=slice(30, None))
    e["rojo_que_casi_no_esta"] = pintar(imagen(ROJO), (30, 30, 220), columnas=slice(10, None))
    return e


def escenas_aleatorias(rng, cantidad=24, alto=54, ancho=96):
    """Escenas variadas a 96×54: bloques, degradés, ruido, bandas negras y casi grises."""
    e = {}
    for i in range(cantidad):
        tipo = i % 6
        img = np.zeros((alto, ancho, 4), np.uint8)
        if tipo == 0:  # bloques de colores
            for _ in range(rng.integers(2, 7)):
                y0, x0 = rng.integers(0, alto - 4), rng.integers(0, ancho - 4)
                y1, x1 = rng.integers(y0 + 2, alto + 1), rng.integers(x0 + 2, ancho + 1)
                pintar(img, tuple(int(c) for c in rng.integers(0, 256, 3)), slice(y0, y1), slice(x0, x1))
        elif tipo == 1:  # degradé horizontal entre dos colores
            a, b = rng.integers(0, 256, 3), rng.integers(0, 256, 3)
            t = np.linspace(0, 1, ancho)[None, :, None]
            rgb = (a[None, None, :] * (1 - t) + b[None, None, :] * t).repeat(alto, axis=0)
            img[:, :, 2], img[:, :, 1], img[:, :, 0] = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        elif tipo == 2:  # ruido sobre un color base
            base = rng.integers(0, 256, 3)
            ruido = rng.normal(0, 40, (alto, ancho, 3))
            rgb = np.clip(base + ruido, 0, 255)
            img[:, :, 2], img[:, :, 1], img[:, :, 0] = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        elif tipo == 3:  # película con bandas negras arriba y abajo
            pintar(img, tuple(int(c) for c in rng.integers(30, 256, 3)), slice(7, alto - 7))
            pintar(img, tuple(int(c) for c in rng.integers(0, 256, 3)), slice(15, 35), slice(20, 60))
        elif tipo == 4:  # casi gris con un toque de color
            g = int(rng.integers(20, 240))
            pintar(img, (g, g, g))
            pintar(img, tuple(int(c) for c in rng.integers(0, 256, 3)),
                   slice(0, int(rng.integers(2, 12))), slice(0, int(rng.integers(2, 20))))
        else:  # oscuro
            rgb = rng.integers(0, 40, (alto, ancho, 3))
            img[:, :, 2], img[:, :, 1], img[:, :, 0] = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        img[:, :, 3] = 255
        e[f"aleatoria_{i:02d}_tipo{tipo}"] = img
    return e


def fixture_color_de_pantalla(rng):
    escenas = escenas_de_las_pruebas() | escenas_aleatorias(rng)
    casos = []
    for nombre, img in escenas.items():
        alto, ancho = img.shape[:2]
        salidas = []
        for modo in ("dominante", "promedio"):
            for previo in (None, 0.0, 1 / 3, 2 / 3, 0.95):
                h, s, luz = color_de_pantalla(img, modo, previo)
                salidas.append({"mode": modo, "tono_previo": previo, "h": f(h), "s": f(s), "luz": f(luz)})
        casos.append({"name": nombre, "width": ancho, "height": alto, "bgra_b64": b64(img),
                      "results": salidas})
    return {"function": "color_de_pantalla", "constants": CONSTANTES, "cases": casos}


def fixture_objetivo_del_foco(rng):
    tablas = {"identidad": {}, "real": CALIBRACION_REAL, "bordes": CALIBRACION_BORDES}
    ajustes = [dict(saturacion=1.3, brillo_min=0.05, brillo_max=1.0),
               dict(saturacion=1.0, brillo_min=0.2, brillo_max=0.6),
               dict(saturacion=2.5, brillo_min=0.0, brillo_max=1.0)]
    casos = []
    for nombre_tabla, tabla in tablas.items():
        cal = Calibracion(tabla)
        for aj in ajustes:
            entradas = [(None, 0.0, 0.0, None), (None, 0.0, 0.05, (0.6, 0.8, 0.3)),
                        (None, 0.0, 0.5, (0.6, 0.8, 0.3)), (0.0, 1.0, 1.0, None)]
            entradas += [(float(rng.random()), float(rng.random()), float(rng.random()),
                          None if rng.random() < 0.3 else tuple(float(x) for x in rng.random(3)))
                         for _ in range(14)]
            for h, s, luz, actual in entradas:
                r = objetivo_del_foco(h, s, luz, actual, cal, aj["saturacion"], aj["brillo_min"], aj["brillo_max"])
                casos.append({"calibration": nombre_tabla, **aj, "h": h, "s": s, "luz": luz,
                              "actual": list(actual) if actual else None, "result": [f(x) for x in r]})
    return {"function": "objetivo_del_foco",
            "calibrations": {k: {str(t): v for t, v in tab.items()} for k, tab in tablas.items()},
            "cases": casos}


def fixture_calibracion(rng):
    tablas = {"identidad": {}, "real": CALIBRACION_REAL, "bordes": CALIBRACION_BORDES}
    casos = []
    hs = [i / 72 for i in range(72)] + [0.9999, 1e-6] + [float(x) for x in rng.random(10)]
    for nombre, tabla in tablas.items():
        cal = Calibracion(tabla)
        for h in hs:
            for s in (0.0, 0.5, 1.0):
                th, ts = cal.aplicar(h, s)
                casos.append({"calibration": nombre, "h": h, "s": s, "result": [f(th), f(ts)]})
    return {"function": "Calibracion.aplicar",
            "calibrations": {k: {str(t): v for t, v in tab.items()} for k, tab in tablas.items()},
            "cases": casos}


def fixture_diferencia(rng):
    casos = []
    fijos = [((0.0, 1.0, 1.0), (0.5, 1.0, 1.0)), ((0.0, 0.0, 1.0), (0.5, 0.0, 1.0)),
             ((0.95, 1.0, 0.5), (0.05, 1.0, 0.5)), ((0.3, 0.2, 0.1), (0.3, 0.6, 0.9))]
    pares = fijos + [(tuple(float(x) for x in rng.random(3)), tuple(float(x) for x in rng.random(3)))
                     for _ in range(60)]
    for a, b in pares:
        casos.append({"a": list(a), "b": list(b), "result": f(diferencia(a, b))})
    sep = [{"a": float(a), "b": float(b), "result": f(separacion_tonos(a, b))}
           for a, b in [(0, 359), (10, 350), (180, 0), (90, 270), (45, 44.5)]
           + [tuple(rng.random(2) * 720 - 180) for _ in range(20)]]
    return {"function": "diferencia", "cases": casos, "separacion_tonos": sep}


def fixture_suavizador(rng):
    secuencias = []
    for lento in (0.0, 0.3, 0.05, 1.0):
        for nombre in ("cambio_chico", "corte_de_escena", "opuestos", "desde_blanco", "aleatoria"):
            suav = Suavizador(lento)
            t = 100.0
            pasos = []
            objetivos = {
                "cambio_chico": [(0.10, 0.8, 0.5)] + [(0.12, 0.8, 0.52)] * 15,
                "corte_de_escena": [(0.0, 1.0, 0.8)] + [(0.66, 1.0, 0.3)] * 15,
                "opuestos": [(0.0, 1.0, 1.0)] + [(0.5, 1.0, 1.0)] * 15,
                "desde_blanco": [(0.2, 0.05, 1.0)] + [(0.7, 1.0, 1.0)] * 10,
                "aleatoria": [tuple(float(x) for x in rng.random(3)) for _ in range(25)],
            }[nombre]
            for obj in objetivos:
                t += float(rng.choice([0.016, 0.05, 0.1, 0.25]))
                r = suav.actualizar(obj, t)
                pasos.append({"objetivo": list(obj), "ahora": t, "result": [f(x) for x in r]})
            secuencias.append({"name": nombre, "lento_s": lento, "steps": pasos})
    return {"function": "Suavizador.actualizar", "cambio_grande": Suavizador.CAMBIO_GRANDE,
            "sequences": secuencias}


def fixture_varios():
    vc = []
    for envio in ("24", "directo", "gradual"):
        for h, s, v in [(0, 0, 0.01), (1, 1, 1), (0.5, 0.5, 0.5), (0.6667, 1.0, 0.5), (0.123, 0.456, 0.789)]:
            dp, valor = valor_color(envio, h, s, v)
            vc.append({"envio": envio, "h": h, "s": s, "v": v, "dp": dp, "value": valor})
    monitores = []
    for pedido, integrados in [(None, []), (None, [True]), (None, [True, False]), (None, [False, True]),
                               (2, [True, False]), (3, [True, False]), (1, [True, False]), (None, [True, True])]:
        monitores.append({"pedido": pedido, "integrados": integrados, "result": elegir_monitor(pedido, integrados)})
    estado_color = {"20": True, "21": "colour", "22": 1000, "23": 1000, "24": "007c013c02d8", "26": 0}
    estado_blanco = {"20": True, "21": "white", "24": "00df03e8000a", "22": 1000, "23": 396}
    guardado = {"20": False, "21": "white", "22": 500}
    originales = []
    for dps, g in [(estado_blanco, None), (estado_color, guardado), (estado_blanco, guardado), (estado_color, None)]:
        originales.append({"dps": dps, "guardado": g, "result": elegir_original(dps, g)})
    return {"valor_color": vc, "elegir_monitor": monitores, "elegir_original": originales}


def main():
    rng = np.random.default_rng(SEMILLA)
    random.seed(SEMILLA)
    SALIDA.mkdir(parents=True, exist_ok=True)
    fixtures = {
        "color_de_pantalla": fixture_color_de_pantalla(rng),
        "objetivo_del_foco": fixture_objetivo_del_foco(rng),
        "calibracion": fixture_calibracion(rng),
        "diferencia": fixture_diferencia(rng),
        "suavizador": fixture_suavizador(rng),
        "varios": fixture_varios(),
    }
    for nombre, datos in fixtures.items():
        ruta = SALIDA / f"{nombre}.json"
        datos = {"source": "ambilight.py (~/Documents/Open RGB)", "seed": SEMILLA, **datos}
        ruta.write_text(json.dumps(datos, indent=1, ensure_ascii=False) + "\n")
        print("escrito", ruta.relative_to(SALIDA.parents[1]), f"({ruta.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
