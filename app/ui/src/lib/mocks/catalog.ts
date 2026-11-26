// Copia del catálogo de `crates/openlight-types/src/settings.rs` para los mocks.
// La UI real lo recibe de `settings_get`; si el catálogo de Rust cambia, actualizá esto también
// (la prueba `catalog.test.ts` verifica que esté completo y que los defaults estén en rango).
import type { SettingDef, SettingId, SettingKind, SettingLevel, SettingUnit, SettingValue, Settings } from "../bindings";

function def(id: SettingId, kind: SettingKind, value: SettingValue, level: SettingLevel): SettingDef {
  return { id, kind, default: value, level, title_key: `settings.${id}.title`, help_key: `settings.${id}.help` };
}
const num = (min: number, max: number, step: number, unit: SettingUnit): SettingKind => ({ kind: "number", min, max, step, unit });
const choice = (...options: string[]): SettingKind => ({ kind: "choice", options });

export const CATALOG: SettingDef[] = [
  def("color_mode", choice("dominant", "average"), "dominant", "basic"),
  def("saturation", num(0.5, 2.5, 0.05, "factor"), 1.3, "basic"),
  def("brightness_min", num(0, 1, 0.01, "fraction"), 0.05, "basic"),
  def("brightness_max", num(0.05, 1, 0.01, "fraction"), 1.0, "basic"),
  def("smoothing_ms", num(0, 2000, 10, "milliseconds"), 300, "basic"),
  def("send_channel", choice("colour", "control_direct", "control_gradual"), "colour", "advanced"),
  def("capture_fps", num(5, 60, 1, "fps"), 20, "advanced"),
  def("max_sends_per_s", num(1, 20, 1, "per_second"), 8, "advanced"),
  def("send_threshold", num(0, 0.1, 0.005, "fraction"), 0.01, "advanced"),
  def("black_bar", num(0, 0.3, 0.01, "fraction"), 0.06, "advanced"),
  def("grey_threshold", num(0, 0.2, 0.005, "fraction"), 0.02, "advanced"),
  def("dark_screen", num(0, 0.5, 0.01, "fraction"), 0.12, "advanced"),
  def("hue_stickiness", num(1, 3, 0.05, "factor"), 1.4, "advanced"),
  def("no_screen_wait_s", num(5, 300, 1, "seconds"), 20, "advanced"),
  def("heartbeat_s", num(3, 30, 1, "seconds"), 10, "expert"),
  def("dead_after_s", num(5, 120, 1, "seconds"), 30, "expert"),
  def("retry_every_s", num(1, 30, 1, "seconds"), 3, "expert"),
];

export function defaultSettings(): Settings {
  return Object.fromEntries(CATALOG.map((d) => [d.id, d.default])) as unknown as Settings;
}
