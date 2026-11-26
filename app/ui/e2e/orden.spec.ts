import { expect, test, type Page } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

/** Lo que quedó guardado en el backend falso, en orden. */
const saved = (page: Page) =>
  page.evaluate(() =>
    (window as any).__openlightMock.state.devices.map((d: any) => ({ name: d.name, zones: d.zones.map((z: any) => `${z.display}:${z.preset}`) })),
  );

test("distribuciones: con cuatro focos se reparten las cuatro esquinas, en el orden del panel", async ({ page }) => {
  await open(page, "four", "devices");
  const layouts = page.getByRole("group", { name: "Distribución para todos los focos" });
  // Solo las que coinciden con la cantidad de focos.
  await expect(layouts.getByRole("button")).toHaveText([/Cuatro esquinas$/]);
  await expect(layouts).toContainText("1 Foco living, 2 Lámpara escritorio, 3 Foco repisa, 4 Foco techo");

  const corners = layouts.getByRole("button", { name: "Cuatro esquinas" });
  await expect(corners).toHaveAttribute("aria-pressed", "false");
  await corners.click();
  await expect(corners).toHaveAttribute("aria-pressed", "true");
  const display = "69733382";
  await expect
    .poll(async () => (await saved(page)).map((d: { zones: string[] }) => d.zones))
    .toEqual([[`${display}:top_left`], [`${display}:top_right`], [`${display}:bottom_left`], [`${display}:bottom_right`]]);

  // Después se ajusta cada uno a mano con el editor de siempre.
  const presets = page.getByRole("group", { name: "Atajos de zona para Foco living" });
  await expect(presets.getByRole("button", { name: "Arriba izquierda" })).toHaveAttribute("aria-pressed", "true");
  await presets.getByRole("button", { name: "Centro" }).click();
  await expect(corners).toHaveAttribute("aria-pressed", "false");
  await expect.poll(async () => (await saved(page))[0].zones).toEqual([`${display}:center`]);
  await expectAccessible(page, "distribuciones con cuatro focos");
});

test("distribuciones: con dos focos se ofrecen lados o arriba y abajo, sin tocar otros monitores", async ({ page }) => {
  await open(page, "configured", "devices");
  const layouts = page.getByRole("group", { name: "Distribución para todos los focos" });
  await expect(layouts.getByRole("button")).toHaveText([/Izquierda y derecha$/, /Arriba y abajo$/]);
  await expect(layouts.getByRole("button", { name: "Izquierda y derecha" })).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("radio", { name: /Pantalla integrada/ }).check({ force: true });
  await expect(layouts.getByRole("button", { name: "Izquierda y derecha" })).toHaveAttribute("aria-pressed", "false");
  await layouts.getByRole("button", { name: "Arriba y abajo" }).click();
  await expect
    .poll(async () => (await saved(page)).map((d: { zones: string[] }) => d.zones))
    .toEqual([
      ["69733382:left", "1:top"],
      ["69733382:right", "1:bottom"],
    ]);
  await expectAccessible(page, "distribuciones con dos focos");
});
