#!/usr/bin/env python3
"""Genera los SVG de marca de OpenLight: el icono de la app (siempre oscuro) y el logo con
palabra en versión oscura y clara. El texto se convierte a trazados con la misma fuente de la
UI (Plus Jakarta Sans), así los SVG se ven igual en cualquier lado sin depender de fuentes.

    python3 tools/brand/build_brand.py

Escribe app/src-tauri/icons/source.svg (de ahí salen los PNG/ICNS/ICO con `pnpm tauri icon`)
y app/ui/src/assets/logo-{dark,light}.svg.
"""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = Path(__file__).resolve().parents[2]
FONT = ROOT / "app/ui/node_modules/@fontsource-variable/plus-jakarta-sans/files/plus-jakarta-sans-latin-wght-normal.woff2"
GRAD = [(0, "#00c8ff"), (0.30, "#5a2bff"), (0.62, "#ff2bf0"), (1, "#ff8a1a")]
NAME = "OpenLight"


def gradient(gid, x0, x1):
    stops = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in GRAD)
    return f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{x0}" y1="0" x2="{x1}" y2="0">{stops}</linearGradient>'


def mark(uid, cx, cy, w, outline, interior, glow=1.0, veil="#07071a"):
    """Pantalla 16:9 vacía con el resplandor de un Ambilight. Devuelve (defs, cuerpo)."""
    h = round(w * 9 / 16, 2)
    x, y = round(cx - w / 2, 2), round(cy - h / 2, 2)
    s = w / 560
    rx = round(46 * s, 2)
    pad = round(w * 0.7)
    region = f'x="{x - pad}" y="{y - pad}" width="{w + 2 * pad}" height="{h + 2 * pad}" filterUnits="userSpaceOnUse"'
    defs = (
        gradient(f"g{uid}", x, x + w)
        + f'<filter id="big{uid}" {region}><feGaussianBlur stdDeviation="{round(44 * s, 2)}"/></filter>'
        + f'<filter id="mid{uid}" {region}><feGaussianBlur stdDeviation="{round(15 * s, 2)}"/></filter>'
        + f'<linearGradient id="v{uid}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{veil}" stop-opacity="0.8"/><stop offset="0.7" stop-color="{veil}" stop-opacity="0"/></linearGradient>'
        + f'<clipPath id="in{uid}"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"/></clipPath>'
    )
    rect = f'x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"'
    body = (
        (f'<rect {rect} fill="{interior}"/>' if interior else "")
        + f'<g clip-path="url(#in{uid})"><rect {rect} fill="none" stroke="url(#g{uid})" stroke-width="{round(150 * s, 2)}" filter="url(#big{uid})" opacity="{0.95 * glow:.2f}"/><rect {rect} fill="url(#v{uid})"/></g>'
        + f'<rect {rect} fill="none" stroke="url(#g{uid})" stroke-width="{round(90 * s, 2)}" filter="url(#big{uid})" opacity="{0.95 * glow:.2f}"/>'
        + f'<rect {rect} fill="none" stroke="url(#g{uid})" stroke-width="{round(26 * s, 2)}" filter="url(#mid{uid})"/>'
        + f'<rect {rect} fill="none" stroke="{outline}" stroke-width="{round(22 * s, 2)}"/>'
    )
    return defs, body


def text_path(fill, size, x, baseline):
    font = TTFont(FONT)
    font = instancer.instantiateVariableFont(font, {"wght": 640})
    gs, cmap, upm = font.getGlyphSet(), font.getBestCmap(), font["head"].unitsPerEm
    k = size / upm
    pen_x, d = 0, []
    for ch in NAME:
        g = cmap[ord(ch)]
        pen = SVGPathPen(gs)
        gs[g].draw(TransformPen(pen, (k, 0, 0, -k, x + pen_x * k, baseline)))
        d.append(pen.getCommands())
        pen_x += gs[g].width
    return f'<path fill="{fill}" d="{" ".join(d)}"/>', pen_x * k


def svg(vb_w, vb_h, defs, body, extra=""):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vb_w} {vb_h}"{extra}><defs>{defs}</defs>{body}</svg>\n'


def icon():
    d, b = mark("i", 512, 512, 560, "#ffffff", "#07071a")
    d += '<clipPath id="sq"><rect x="64" y="64" width="896" height="896" rx="210"/></clipPath>'
    bg = '<rect x="64" y="64" width="896" height="896" rx="210" fill="#0a0a1f"/>'
    return svg(1024, 1024, d, bg + f'<g clip-path="url(#sq)">{b}</g>')


def logo(mode):
    dark = mode == "dark"
    text_fill = "#f4f5f7" if dark else "#16181d"
    outline = "#ffffff" if dark else "#16181d"
    interior = None
    d, b = mark(mode[0], 240, 170, 300, outline, interior, glow=1.0 if dark else 0.85, veil="#07071a" if dark else "#ffffff")
    t, tw = text_path(text_fill, 150, 460, 170 + 52)
    return svg(round(460 + tw + 90), 340, d, b + t)


if __name__ == "__main__":
    out = ROOT / "app/ui/src/assets"
    (ROOT / "app/src-tauri/icons/source.svg").write_text(icon())
    for m in ("dark", "light"):
        (out / f"logo-{m}.svg").write_text(logo(m))
    print("ok")
