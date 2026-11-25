// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import { CATALOG } from "../mocks/catalog";
import { setLocale } from "../i18n/index.svelte";
import SettingControl from "./SettingControl.svelte";

setLocale("es");

describe("SettingControl: un control por cada ajuste del catálogo", () => {
  it.each(CATALOG.map((d) => [d.id, d] as const))("%s", (_id, def) => {
    const { unmount } = render(SettingControl, { def, value: def.default, onchange: () => {}, onreset: () => {} });
    if (def.kind.kind === "number") {
      const slider = screen.getByRole("slider");
      expect(slider.getAttribute("min")).toBe(String(def.kind.min));
      expect(slider.getAttribute("max")).toBe(String(def.kind.max));
      expect(slider.getAttribute("step")).toBe(String(def.kind.step));
    } else {
      expect(screen.getAllByRole("radio")).toHaveLength(def.kind.options.length);
    }
    expect(screen.getByText(/^Por defecto:/)).toBeTruthy();
    unmount();
  });

  it("no baja del piso ni sube del tope que le pasan", async () => {
    const def = CATALOG.find((d) => d.id === "brightness_max")!;
    const onchange = vi.fn();
    render(SettingControl, { def, value: 0.8, floor: 0.4, ceil: 0.9, onchange, onreset: () => {} });
    const slider = screen.getByRole("slider") as HTMLInputElement;
    await fireEvent.input(slider, { target: { value: "0.1" } });
    expect(slider.value).toBe("0.4");
    expect(onchange).toHaveBeenLastCalledWith(0.4);
    await fireEvent.input(slider, { target: { value: "1" } });
    expect(slider.value).toBe("0.9");
  });

  it("cambia y restaura", async () => {
    const def = CATALOG.find((d) => d.id === "color_mode")!;
    const onchange = vi.fn();
    const onreset = vi.fn();
    render(SettingControl, { def, value: "average", onchange, onreset });
    await fireEvent.click(screen.getByRole("radio", { name: "Dominante" }));
    expect(onchange).toHaveBeenCalledWith("dominant");
    await fireEvent.click(screen.getByRole("button", { name: "Restaurar Modo de color" }));
    expect(onreset).toHaveBeenCalled();
  });
});
