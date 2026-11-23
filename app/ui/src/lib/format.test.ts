import { describe, expect, it } from "vitest";
import { formatSetting, hsvToRgb, screenCss } from "./format";
import { hueKey } from "./hue";
import { constrain, presetZone } from "./zones";
import { CATALOG } from "./mocks/catalog";

const def = (id: string) => CATALOG.find((d) => d.id === id)!;

describe("formato", () => {
  it("muestra cada unidad como corresponde", () => {
    expect(formatSetting(0.05, def("brightness_min").kind, "es")).toBe("5 %");
    expect(formatSetting(1.3, def("saturation").kind, "es")).toBe("×1,30");
    expect(formatSetting(1.3, def("saturation").kind, "en")).toBe("×1.30");
    expect(formatSetting(300, def("smoothing_ms").kind, "es")).toBe("300 ms");
    expect(formatSetting(20, def("capture_fps").kind, "es")).toBe("20 fps");
    expect(formatSetting(8, def("max_sends_per_s").kind, "es")).toBe("8/s");
    expect(formatSetting(0.015, def("send_threshold").kind, "es")).toBe("1,5 %");
  });

  it("convierte HSV a RGB", () => {
    expect(hsvToRgb(0, 1, 1)).toEqual([255, 0, 0]);
    expect(hsvToRgb(1 / 3, 1, 1)).toEqual([0, 255, 0]);
    expect(hsvToRgb(2 / 3, 1, 0.5)).toEqual([0, 0, 128]);
  });

  it("una zona sin tono se muestra gris", () => {
    expect(screenCss({ h: null, s: 0, luz: 0.5 })).toBe("hsl(0 0% 50%)");
  });

  it("nombra los 12 tonos de calibrar.py", () => {
    expect(hueKey(0)).toBe("hue.red");
    expect(hueKey(29)).toBe("hue.orange");
    expect(hueKey(359)).toBe("hue.red");
    expect(hueKey(240)).toBe("hue.blue");
  });

  it("las zonas no se salen del monitor", () => {
    expect(constrain({ x: 0.9, y: -0.2, w: 0.5, h: 2 })).toEqual({ x: 0.5, y: 0, w: 0.5, h: 1 });
    expect(constrain({ x: 0, y: 0, w: 0.01, h: 0.01 })).toEqual({ x: 0, y: 0, w: 0.1, h: 0.1 });
    expect(presetZone("right", "d").rect).toEqual({ x: 0.5, y: 0, w: 0.5, h: 1 });
  });
});
