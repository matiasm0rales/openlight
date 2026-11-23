import { describe, expect, it } from "vitest";
import { CATALOG, defaultSettings } from "./mocks/catalog";
import { clampToDef, groupByLevel, isDefault, valueIn, visibleDefs } from "./settings";
import type { SettingsSnapshot } from "./bindings";

const snapshot = (): SettingsSnapshot => ({
  catalog: CATALOG,
  effective: { ...defaultSettings(), saturation: 1.6 },
  global: defaultSettings(),
  devices: [{ device: "foco", overrides: { brightness_max: 0.4 } }],
  active_profile: null,
});

describe("ajustes generados desde el catálogo", () => {
  it("cada nivel muestra los suyos y los anteriores", () => {
    expect(visibleDefs(CATALOG, "basic").every((d) => d.level === "basic")).toBe(true);
    expect(visibleDefs(CATALOG, "advanced").map((d) => d.level)).not.toContain("expert");
    expect(visibleDefs(CATALOG, "expert")).toHaveLength(CATALOG.length);
  });

  it("agrupa por nivel sin perder ajustes", () => {
    const groups = groupByLevel(CATALOG);
    expect(groups.map((g) => g.level)).toEqual(["basic", "advanced", "expert"]);
    expect(groups.flatMap((g) => g.defs)).toHaveLength(CATALOG.length);
  });

  it("un foco hereda lo global salvo lo que pisa", () => {
    const s = snapshot();
    expect(valueIn(s, { kind: "device", id: "foco" }, "brightness_max", [])).toEqual({ value: 0.4, inherited: false });
    expect(valueIn(s, { kind: "device", id: "foco" }, "saturation", [])).toEqual({ value: 1.6, inherited: true });
    expect(valueIn(s, { kind: "global" }, "saturation", [])).toEqual({ value: 1.6, inherited: false });
  });

  it("un perfil hereda de los globales sin perfil", () => {
    const s = snapshot();
    const profiles = [{ id: "movies", values: { smoothing_ms: 500 } }];
    expect(valueIn(s, { kind: "profile", id: "movies" }, "smoothing_ms", profiles).value).toBe(500);
    expect(valueIn(s, { kind: "profile", id: "movies" }, "saturation", profiles)).toEqual({ value: 1.3, inherited: true });
  });

  it("ajusta al rango y al paso del catálogo", () => {
    const sat = CATALOG.find((d) => d.id === "saturation")!;
    expect(clampToDef(sat, 9)).toBe(2.5);
    expect(clampToDef(sat, 0.1)).toBe(0.5);
    expect(clampToDef(sat, 1.33)).toBe(1.35);
    const fps = CATALOG.find((d) => d.id === "capture_fps")!;
    expect(clampToDef(fps, 20.4)).toBe(20);
  });

  it("los defaults del catálogo están en rango y se reconocen como default", () => {
    for (const def of CATALOG) {
      expect(isDefault(def, def.default)).toBe(true);
      if (def.kind.kind === "number") {
        expect(def.default as number).toBeGreaterThanOrEqual(def.kind.min ?? 0);
        expect(def.default as number).toBeLessThanOrEqual(def.kind.max ?? 0);
      } else {
        expect(def.kind.options).toContain(def.default);
      }
    }
  });
});
