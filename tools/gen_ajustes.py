#!/usr/bin/env python3
"""Genera la tabla de ajustes de `docs/guia.md` desde el catálogo.

Lee el catálogo de `crates/openlight-types/src/settings.rs` (ids, rangos, valores por defecto y
niveles) y los textos de `app/ui/src/lib/i18n/es.json`, y reemplaza lo que hay entre las marcas
`ajustes:inicio` y `ajustes:fin` de la guía.

    python3 tools/gen_ajustes.py           # reescribe la guía
    python3 tools/gen_ajustes.py --check   # falla si la guía quedó desactualizada
"""

import json
import math
import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
CATALOGO = RAIZ / "crates/openlight-types/src/settings.rs"
TEXTOS = RAIZ / "app/ui/src/lib/i18n/es.json"
GUIA = RAIZ / "docs/guia.md"
NIVELES = ["Basic", "Advanced", "Expert"]

DEFINICION = re.compile(
    r"SettingDef::new\(\s*I::(?P<id>\w+),\s*"
    r"(?:choice\(&\[(?P<opciones>[^\]]*)\]\)|number\((?P<numero>[^)]*)\)),\s*"
    r"[cn]\((?P<defecto>[^)]*)\),\s*"
    r"(?P<nivel>\w+),?\s*\)"
)


def serpiente(nombre):
    """`MaxSendsPerS` → `max_sends_per_s`."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", nombre).lower()


def leer_catalogo():
    fuente = CATALOGO.read_text(encoding="utf-8")
    ajustes = [m.groupdict() for m in DEFINICION.finditer(fuente)]
    todos = re.search(r"pub const ALL: \[Self; (\d+)\]", fuente)
    if not todos or len(ajustes) != int(todos.group(1)):
        sys.exit(f"leí {len(ajustes)} ajustes y no coincide con SettingId::ALL: revisá el script")
    return ajustes


def formatear(valor, paso, unidad):
    """Como `formatSetting` de la interfaz, en español."""
    def numero(n, decimales):
        return f"{n:.{decimales}f}".replace(".", ",")

    if unidad == "Fraction":
        decimales = 0 if paso >= 1 else min(3, math.ceil(-math.log10(paso) - 1e-9))
        return f"{numero(valor * 100, max(0, decimales - 2))} %"
    if unidad == "Factor":
        return f"×{numero(valor, 2)}"
    sufijo = {"Milliseconds": " ms", "Seconds": " s", "Fps": " fps", "PerSecond": "/s"}[unidad]
    return f"{numero(valor, 0)}{sufijo}"


def generar():
    textos = json.loads(TEXTOS.read_text(encoding="utf-8"))["settings"]
    ajustes = leer_catalogo()
    salida = []
    for nivel in NIVELES:
        salida += [
            f"### Nivel {textos['level'][nivel.lower()].lower()}",
            "",
            "| Ajuste | Qué hace | Por defecto | Valores |",
            "|---|---|---|---|",
        ]
        for a in (a for a in ajustes if a["nivel"] == nivel):
            ident = serpiente(a["id"])
            texto = textos[ident]
            if a["opciones"] is not None:
                opciones = re.findall(r'"(\w+)"', a["opciones"])
                defecto = texto["option"][a["defecto"].strip('"')]
                valores = ", ".join(texto["option"][o] for o in opciones)
            else:
                minimo, maximo, paso, unidad = (p.strip() for p in a["numero"].split(","))
                unidad = unidad.removeprefix("U::")
                paso = float(paso)
                defecto = formatear(float(a["defecto"]), paso, unidad)
                valores = " a ".join(
                    formatear(float(v), paso, unidad) for v in (minimo, maximo)
                )
            salida.append(
                f"| **{texto['title']}** (`{ident}`) | {texto['help']} | {defecto} | {valores} |"
            )
        salida.append("")
    return "\n".join(salida)


def main():
    guia = GUIA.read_text(encoding="utf-8")
    marcas = re.compile(r"(<!-- ajustes:inicio[^\n]*-->\n).*?(<!-- ajustes:fin -->)", re.S)
    nueva, veces = marcas.subn(lambda m: f"{m[1]}\n{generar()}\n{m[2]}", guia)
    if veces != 1:
        sys.exit("no encontré las marcas de ajustes en docs/guia.md")
    if "--check" in sys.argv[1:]:
        if nueva != guia:
            sys.exit("docs/guia.md está desactualizada: corré python3 tools/gen_ajustes.py")
        return
    GUIA.write_text(nueva, encoding="utf-8")


if __name__ == "__main__":
    main()
