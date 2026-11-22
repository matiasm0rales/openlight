"""Importa el script original (solo lectura) para usarlo como referencia en los generadores.

Importar `ambilight.py` no escribe nada: solo define funciones y carga CoreGraphics (macOS).
"""

import sys
from pathlib import Path

ORIGEN = Path.home() / "Documents/Open RGB"
if not (ORIGEN / "ambilight.py").exists():
    sys.exit(f"No encuentro {ORIGEN / 'ambilight.py'}: los generadores necesitan el script original.")
sys.path.insert(0, str(ORIGEN))

import ambilight  # noqa: E402

from ambilight import (  # noqa: E402,F401
    Calibracion,
    Suavizador,
    color_de_pantalla,
    diferencia,
    elegir_monitor,
    elegir_original,
    objetivo_del_foco,
    separacion_tonos,
    valor_color,
    zona_dominante,
)

CONSTANTES = {
    "BANDA_NEGRA": ambilight.BANDA_NEGRA,
    "SIN_COLOR": ambilight.SIN_COLOR,
    "PANTALLA_OSCURA": ambilight.PANTALLA_OSCURA,
    "VENTAJA_TONO_PREVIO": ambilight.VENTAJA_TONO_PREVIO,
    "UMBRAL_ENVIO": ambilight.UMBRAL_ENVIO,
    "CAMBIO_GRANDE": Suavizador.CAMBIO_GRANDE,
}
