// Rectángulos de los atajos de zona (según la documentación de `ZonePreset` en el contrato).
import type { NormRect, Zone, ZonePreset } from "./bindings";

export const PRESETS: Exclude<ZonePreset, "custom">[] = ["full", "left", "right", "top", "bottom", "top_left", "top_right", "bottom_left", "bottom_right", "center"];

export const PRESET_RECTS: Record<Exclude<ZonePreset, "custom">, Required<{ [K in keyof NormRect]: number }>> = {
  full: { x: 0, y: 0, w: 1, h: 1 },
  left: { x: 0, y: 0, w: 0.5, h: 1 },
  right: { x: 0.5, y: 0, w: 0.5, h: 1 },
  top: { x: 0, y: 0, w: 1, h: 0.5 },
  bottom: { x: 0, y: 0.5, w: 1, h: 0.5 },
  top_left: { x: 0, y: 0, w: 0.5, h: 0.5 },
  top_right: { x: 0.5, y: 0, w: 0.5, h: 0.5 },
  bottom_left: { x: 0, y: 0.5, w: 0.5, h: 0.5 },
  bottom_right: { x: 0.5, y: 0.5, w: 0.5, h: 0.5 },
  center: { x: 0.25, y: 0.25, w: 0.5, h: 0.5 },
};

export function presetZone(preset: Exclude<ZonePreset, "custom">, display: string): Zone {
  return { display, rect: { ...PRESET_RECTS[preset] }, preset };
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function rectOf(z: Zone | undefined): Rect {
  const r = z?.rect;
  return { x: r?.x ?? 0, y: r?.y ?? 0, w: r?.w ?? 1, h: r?.h ?? 1 };
}

// ---------- Distribuciones: el monitor repartido entre todos los focos ----------

export type LayoutId = "full" | "sides" | "stacked" | "top_and_two" | "columns" | "corners";

type Cell = { preset: ZonePreset; rect: Rect };
const cell = (preset: Exclude<ZonePreset, "custom">): Cell => ({ preset, rect: PRESET_RECTS[preset] });
const column = (x: number, w: number): Cell => ({ preset: "custom", rect: { x, y: 0, w, h: 1 } });

/** Los lugares de cada distribución, en el orden en que se reparten: el del panel. */
export const LAYOUTS: Record<LayoutId, Cell[]> = {
  full: [cell("full")],
  sides: [cell("left"), cell("right")],
  stacked: [cell("top"), cell("bottom")],
  top_and_two: [cell("top"), cell("bottom_left"), cell("bottom_right")],
  // Tercios con los tres decimales de `constrain`: suman justo el ancho del monitor.
  columns: [column(0, 0.333), column(0.333, 0.334), column(0.667, 0.333)],
  corners: [cell("top_left"), cell("top_right"), cell("bottom_left"), cell("bottom_right")],
};

/** Las distribuciones que sirven para `count` focos. */
export function layoutsFor(count: number): LayoutId[] {
  return (Object.keys(LAYOUTS) as LayoutId[]).filter((id) => LAYOUTS[id].length === count);
}

/** La zona de cada foco en `display`, o nada si la distribución es para otra cantidad. */
export function layoutZones(layout: LayoutId, deviceIds: string[], display: string): { device: string; zone: Zone }[] {
  const cells = LAYOUTS[layout];
  if (cells.length !== deviceIds.length) return [];
  return deviceIds.map((device, i) => ({ device, zone: { display, preset: cells[i].preset, rect: { ...cells[i].rect } } }));
}

/** Si en `display` los focos, en este orden, ya están como en la distribución. */
export function matchesLayout(layout: LayoutId, devices: { zones: Zone[] }[], display: string): boolean {
  const cells = LAYOUTS[layout];
  if (cells.length !== devices.length) return false;
  return devices.every((d, i) => {
    const here = d.zones.filter((z) => z.display === display);
    if (here.length !== 1) return false;
    const rect = rectOf(here[0]);
    return (["x", "y", "w", "h"] as const).every((k) => Math.abs(rect[k] - cells[i].rect[k]) < 0.001);
  });
}

const MIN = 0.1;

/** Mueve o agranda un rectángulo sin salirse del monitor y sin achicarlo de más. */
export function constrain(r: Rect): Rect {
  const w = Math.min(1, Math.max(MIN, r.w));
  const h = Math.min(1, Math.max(MIN, r.h));
  const x = Math.min(1 - w, Math.max(0, r.x));
  const y = Math.min(1 - h, Math.max(0, r.y));
  const round = (n: number) => Math.round(n * 1000) / 1000;
  return { x: round(x), y: round(y), w: round(w), h: round(h) };
}
