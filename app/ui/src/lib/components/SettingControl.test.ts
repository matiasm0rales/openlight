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
