// Rectángulos de los atajos de zona (según la documentación de `ZonePreset` en el contrato).
import type { NormRect, Zone, ZonePreset } from "./bindings";

export const PRESETS: Exclude<ZonePreset, "custom">[] = ["full", "left", "right", "top", "bottom", "center"];

export const PRESET_RECTS: Record<Exclude<ZonePreset, "custom">, Required<{ [K in keyof NormRect]: number }>> = {
  full: { x: 0, y: 0, w: 1, h: 1 },
  left: { x: 0, y: 0, w: 0.5, h: 1 },
  right: { x: 0.5, y: 0, w: 0.5, h: 1 },
  top: { x: 0, y: 0, w: 1, h: 0.5 },
  bottom: { x: 0, y: 0.5, w: 1, h: 0.5 },
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
