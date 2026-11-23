// Nombres de los 12 tonos de calibración (los mismos de calibrar.py), como claves i18n.
export const HUE_KEYS = ["red", "orange", "yellow", "lime", "green", "teal", "cyan", "sky", "blue", "violet", "magenta", "pink"] as const;

/** Clave i18n del nombre del tono más cercano (`hue.red`...). `h` en grados. */
export function hueKey(deg: number): string {
  const i = Math.round((((deg % 360) + 360) % 360) / 30) % 12;
  return `hue.${HUE_KEYS[i]}`;
}
