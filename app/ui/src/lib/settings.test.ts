import { describe, expect, it } from "vitest";
import { CATALOG, defaultSettings } from "./mocks/catalog";
import { clampToDef, groupByLevel, isDefault, limitsFor, shadowedBy, valueIn, visibleDefs } from "./settings";
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
    // Los globales se ven como están guardados, no con el perfil activo encima.
    expect(valueIn(s, { kind: "global" }, "saturation", [])).toEqual({ value: 1.3, inherited: false });
  });

  it("dice quién tapa un valor: el perfil activo y los focos con valor propio", () => {
    const s = { ...snapshot(), active_profile: "movies" };
    const profiles = [{ id: "movies", values: { brightness_max: 0.8 } }];
    expect(shadowedBy(s, { kind: "global" }, "brightness_max", profiles)).toEqual({ profile: "movies", devices: ["foco"] });
    expect(shadowedBy(s, { kind: "global" }, "saturation", profiles)).toEqual({ profile: null, devices: [] });
    expect(shadowedBy(s, { kind: "profile", id: "movies" }, "brightness_max", profiles)).toEqual({ profile: null, devices: ["foco"] });
    // Un perfil que no está activo no tapa nada, y lo propio de un foco es lo que manda.
    expect(shadowedBy(s, { kind: "profile", id: "otro" }, "brightness_max", profiles)).toEqual({ profile: null, devices: [] });
    expect(shadowedBy(s, { kind: "device", id: "foco" }, "brightness_max", profiles)).toEqual({ profile: null, devices: [] });
  });

  it("un perfil hereda de los globales sin perfil", () => {
    const s = snapshot();
    const profiles = [{ id: "movies", values: { smoothing_ms: 500 } }];
    expect(valueIn(s, { kind: "profile", id: "movies" }, "smoothing_ms", profiles).value).toBe(500);
    expect(valueIn(s, { kind: "profile", id: "movies" }, "saturation", profiles)).toEqual({ value: 1.3, inherited: true });
  });

  it("el brillo máximo no baja del mínimo del ámbito, ni el mínimo sube del máximo", () => {
    const s = snapshot();
    const scope = { kind: "device", id: "foco" } as const;
    expect(limitsFor(s, scope, "brightness_max", [])).toEqual({ floor: 0.05 });
    expect(limitsFor(s, scope, "brightness_min", [])).toEqual({ ceil: 0.4 });
    expect(limitsFor(s, scope, "saturation", [])).toEqual({});
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
