import { describe, expect, it } from "vitest";
import { PRESETS, PRESET_RECTS, layoutZones, layoutsFor, matchesLayout, presetZone, type LayoutId } from "./zones";

describe("atajos de zona", () => {
  it("las cuatro esquinas son los cuartos del monitor", () => {
    expect(presetZone("top_left", "m")).toEqual({ display: "m", preset: "top_left", rect: { x: 0, y: 0, w: 0.5, h: 0.5 } });
    expect(PRESET_RECTS.top_right).toEqual({ x: 0.5, y: 0, w: 0.5, h: 0.5 });
    expect(PRESET_RECTS.bottom_left).toEqual({ x: 0, y: 0.5, w: 0.5, h: 0.5 });
    expect(PRESET_RECTS.bottom_right).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  });

  it("se ofrecen todos: entero, mitades, esquinas y centro", () => {
    expect(PRESETS).toEqual(["full", "left", "right", "top", "bottom", "top_left", "top_right", "bottom_left", "bottom_right", "center"]);
  });

  it("cada atajo queda dentro del monitor", () => {
    for (const preset of PRESETS) {
      const r = PRESET_RECTS[preset];
      expect(r.x >= 0 && r.y >= 0 && r.w > 0 && r.h > 0 && r.x + r.w <= 1 && r.y + r.h <= 1, preset).toBe(true);
    }
  });
});

const ALL: [LayoutId, number][] = [
  ["full", 1],
  ["sides", 2],
  ["stacked", 2],
  ["top_and_two", 3],
  ["columns", 3],
  ["corners", 4],
];
const ids = (n: number) => ["a", "b", "c", "d", "e"].slice(0, n);
const presets = (layout: LayoutId, n: number) => layoutZones(layout, ids(n), "m").map((x) => [x.device, x.zone.preset]);

describe("distribuciones para todos los focos", () => {
  it("solo se ofrecen las que coinciden con la cantidad de focos", () => {
    expect([0, 1, 2, 3, 4, 5].map(layoutsFor)).toEqual([[], ["full"], ["sides", "stacked"], ["top_and_two", "columns"], ["corners"], []]);
  });

  it("reparte los lugares en el orden de los focos", () => {
    expect(presets("full", 1)).toEqual([["a", "full"]]);
    expect(presets("sides", 2)).toEqual([["a", "left"], ["b", "right"]]);
    expect(presets("stacked", 2)).toEqual([["a", "top"], ["b", "bottom"]]);
    expect(presets("top_and_two", 3)).toEqual([["a", "top"], ["b", "bottom_left"], ["c", "bottom_right"]]);
    expect(presets("corners", 4)).toEqual([["a", "top_left"], ["b", "top_right"], ["c", "bottom_left"], ["d", "bottom_right"]]);
    expect(layoutZones("sides", ["b", "a"], "m").map((x) => x.device)).toEqual(["b", "a"]);
  });

  it("las tres columnas son tercios a mano, sin huecos ni encimarse", () => {
    const zones = layoutZones("columns", ids(3), "m");
    expect(zones.map((x) => x.zone)).toEqual([
      { display: "m", preset: "custom", rect: { x: 0, y: 0, w: 0.333, h: 1 } },
      { display: "m", preset: "custom", rect: { x: 0.333, y: 0, w: 0.334, h: 1 } },
      { display: "m", preset: "custom", rect: { x: 0.667, y: 0, w: 0.333, h: 1 } },
    ]);
  });

  it("cada una cubre todo el monitor y entra en el contrato (f32)", () => {
    for (const [layout, n] of ALL) {
      const rects = layoutZones(layout, ids(n), "m").map((x) => x.zone.rect as { x: number; y: number; w: number; h: number });
      expect(rects.reduce((sum, r) => sum + r.w * r.h, 0), layout).toBeCloseTo(1, 6);
      for (const [i, r] of rects.entries()) {
        // La regla de `NormRect::is_valid`, con los números como los ve el backend
        expect(Math.fround(Math.fround(r.x) + Math.fround(r.w)), layout).toBeLessThanOrEqual(1 + 1e-6);
        expect(Math.fround(Math.fround(r.y) + Math.fround(r.h)), layout).toBeLessThanOrEqual(1 + 1e-6);
        for (const o of rects.slice(i + 1)) {
          const overlap = Math.max(0, Math.min(r.x + r.w, o.x + o.w) - Math.max(r.x, o.x)) * Math.max(0, Math.min(r.y + r.h, o.y + o.h) - Math.max(r.y, o.y));
          expect(overlap, layout).toBeCloseTo(0, 6);
        }
      }
    }
  });

  it("con otra cantidad de focos no asigna nada", () => {
    expect(layoutZones("corners", ids(3), "m")).toEqual([]);
    expect(layoutZones("sides", ids(3), "m")).toEqual([]);
    expect(layoutZones("full", [], "m")).toEqual([]);
  });

  it("cada zona es una copia: moverla no cambia el atajo", () => {
    const [first] = layoutZones("sides", ids(2), "m");
    first.zone.rect.x = 0.2;
    expect(PRESET_RECTS.left.x).toBe(0);
    expect(layoutZones("sides", ids(2), "m")[0].zone.rect.x).toBe(0);
  });

  it("reconoce la distribución puesta en un monitor", () => {
    const on = (layout: LayoutId, n: number) => layoutZones(layout, ids(n), "m").map((x) => ({ id: x.device, zones: [x.zone, presetZone("center", "otro")] }));
    for (const [layout, n] of ALL) expect(matchesLayout(layout, on(layout, n), "m"), layout).toBe(true);
    expect(matchesLayout("stacked", on("sides", 2), "m")).toBe(false);
    // Los mismos lugares en otro orden no son la misma distribución
    expect(matchesLayout("sides", on("sides", 2).reverse(), "m")).toBe(false);
    expect(matchesLayout("sides", on("sides", 2), "otro")).toBe(false);
    // Una zona de más en ese monitor tampoco
    const [a, b] = on("sides", 2);
    expect(matchesLayout("sides", [{ ...a, zones: [...a.zones, presetZone("top", "m")] }, b], "m")).toBe(false);
    // El contrato manda f32: lo que vuelve del backend no es idéntico
    const back = on("columns", 3).map((d) => ({ ...d, zones: d.zones.map((z) => ({ ...z, rect: { x: Math.fround(z.rect.x ?? 0), y: z.rect.y, w: Math.fround(z.rect.w ?? 0), h: z.rect.h } })) }));
    expect(matchesLayout("columns", back, "m")).toBe(true);
  });
});
