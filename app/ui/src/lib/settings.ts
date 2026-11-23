// Ayudas para generar la pantalla de Ajustes **desde el catálogo** (`settings_get`).
// Nada acá conoce ajustes concretos: todo sale de `SettingDef`.
import type { SettingDef, SettingId, SettingLevel, SettingScope, SettingValue, SettingsOverride, SettingsSnapshot } from "./bindings";

export const LEVELS: SettingLevel[] = ["basic", "advanced", "expert"];

/** Ajustes visibles hasta un nivel (Avanzado muestra también los básicos). */
export function visibleDefs(catalog: SettingDef[], upTo: SettingLevel): SettingDef[] {
  const max = LEVELS.indexOf(upTo);
  return catalog.filter((d) => LEVELS.indexOf(d.level) <= max);
}

export function groupByLevel(defs: SettingDef[]): { level: SettingLevel; defs: SettingDef[] }[] {
  return LEVELS.map((level) => ({ level, defs: defs.filter((d) => d.level === level) })).filter((g) => g.defs.length > 0);
}

export function overridesFor(snapshot: SettingsSnapshot, scope: SettingScope, profiles: { id: string; values: SettingsOverride }[]): SettingsOverride {
  if (scope.kind === "device") return snapshot.devices.find((d) => d.device === scope.id)?.overrides ?? {};
  if (scope.kind === "profile") return profiles.find((p) => p.id === scope.id)?.values ?? {};
  return {};
}

/** Valor que se ve en un ámbito: lo propio del ámbito, o lo heredado. */
export function valueIn(snapshot: SettingsSnapshot, scope: SettingScope, id: SettingId, profiles: { id: string; values: SettingsOverride }[]): { value: SettingValue; inherited: boolean } {
  const own = overridesFor(snapshot, scope, profiles)[id];
  if (own !== undefined) return { value: own, inherited: false };
  const base = scope.kind === "profile" ? snapshot.global : snapshot.effective;
  return { value: base[id] as SettingValue, inherited: scope.kind !== "global" };
}

export function isDefault(def: SettingDef, value: SettingValue): boolean {
  if (typeof value === "number" && typeof def.default === "number") return Math.abs(value - def.default) < 1e-9;
  return value === def.default;
}

/** Ajusta un número al rango y al paso del catálogo. */
export function clampToDef(def: SettingDef, raw: number): number {
  if (def.kind.kind !== "number") return raw;
  const { min, max, step } = def.kind;
  const lo = min ?? -Infinity;
  const hi = max ?? Infinity;
  const s = step ?? 0;
  let v = Math.min(hi, Math.max(lo, raw));
  if (s > 0) v = Math.round((v - lo) / s) * s + lo;
  return Number(Math.min(hi, Math.max(lo, v)).toFixed(6));
}

export function scopeKey(scope: SettingScope): string {
  return scope.kind === "global" ? "global" : `${scope.kind}:${scope.id}`;
}
