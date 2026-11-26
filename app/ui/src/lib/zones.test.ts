import { describe, expect, it } from "vitest";
import { PRESETS, PRESET_RECTS, presetZone } from "./zones";

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
