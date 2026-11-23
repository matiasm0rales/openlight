// Conversión de colores y formato de valores para mostrar.
import type { Hsv, ScreenColor, SettingKind, SettingUnit, SettingValue } from "./bindings";

/** HSV en 0..1 → `rgb()` de CSS. */
export function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const i = Math.floor(h * 6);
  const f = h * 6 - i;
  const p = v * (1 - s);
  const q = v * (1 - f * s);
  const t = v * (1 - (1 - f) * s);
  const [r, g, b] = [
    [v, t, p],
    [q, v, p],
    [p, v, t],
    [p, q, v],
    [t, p, v],
    [v, p, q],
  ][((i % 6) + 6) % 6];
  return [Math.round(r * 255), Math.round(g * 255), Math.round(b * 255)];
}

export function hsvCss(c: Hsv | null | undefined): string {
  if (!c) return "transparent";
  const [r, g, b] = hsvToRgb(c.h ?? 0, c.s ?? 0, Math.max(0.08, c.v ?? 0));
  return `rgb(${r} ${g} ${b})`;
}

/** Color de la zona de pantalla: sin tono (negro o gris) se muestra como gris con su luz. */
export function screenCss(c: ScreenColor | null | undefined): string {
  if (!c) return "transparent";
  if (c.h === null || c.h === undefined) {
    const l = Math.round(10 + (c.luz ?? 0) * 80);
    return `hsl(0 0% ${l}%)`;
  }
  return hsvCss({ h: c.h, s: c.s ?? 0, v: 0.35 + (c.luz ?? 0) * 0.65 });
}

export function hueCss(deg: number, light = 50): string {
  return `hsl(${((deg % 360) + 360) % 360} 100% ${light}%)`;
}

function decimalsFor(step: number | null): number {
  if (!step || step >= 1) return 0;
  return Math.min(3, Math.max(0, Math.ceil(-Math.log10(step) - 1e-9)));
}

/** Formatea el valor de un ajuste según su unidad (`0.05` → `5 %`, `1.3` → `×1,30`). */
export function formatSetting(value: SettingValue, kind: SettingKind, locale: string): string {
  if (kind.kind !== "number" || typeof value !== "number") return String(value ?? "");
  const unit: SettingUnit = kind.unit;
  const nf = (n: number, d: number) => new Intl.NumberFormat(locale, { minimumFractionDigits: d, maximumFractionDigits: d }).format(n);
  switch (unit) {
    case "fraction": {
      const d = Math.max(0, decimalsFor(kind.step) - 2);
      return `${nf(value * 100, d)} %`;
    }
    case "factor":
      return `×${nf(value, 2)}`;
    case "milliseconds":
      return `${nf(value, 0)} ms`;
    case "seconds":
      return `${nf(value, 0)} s`;
    case "fps":
      return `${nf(value, 0)} fps`;
    case "per_second":
      return `${nf(value, 0)}/s`;
  }
}

export function formatBytes(n: number, locale: string): string {
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: i ? 1 : 0 }).format(n)} ${units[i]}`;
}

export function formatTime(ms: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(ms);
}

/** "hace 3 min" con Intl.RelativeTimeFormat. */
export function formatAgo(ms: number, locale: string, now = Date.now()): string {
  const s = Math.round((ms - now) / 1000);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (Math.abs(s) < 60) return rtf.format(s, "second");
  if (Math.abs(s) < 3600) return rtf.format(Math.round(s / 60), "minute");
  return rtf.format(Math.round(s / 3600), "hour");
}
