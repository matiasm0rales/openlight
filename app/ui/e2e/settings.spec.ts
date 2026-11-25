import { expect, test } from "@playwright/test";
import es from "../src/lib/i18n/es.json" with { type: "json" };
import { CATALOG } from "../src/lib/mocks/catalog";
import { expectAccessible, open } from "./helpers";

const title = (id: string) => (es.settings as Record<string, { title: string }>)[id].title;

test("los ajustes se generan desde el catálogo, por nivel", async ({ page }) => {
  await open(page, "configured", "settings");
  const controls = page.locator("[data-setting]");

  for (const [level, label] of [["basic", "Básico"], ["advanced", "Avanzado"], ["expert", "Experto"]] as const) {
    await page.getByRole("radio", { name: label }).check();
    const upTo = ["basic", "advanced", "expert"].slice(0, ["basic", "advanced", "expert"].indexOf(level) + 1);
    const expected = CATALOG.filter((d) => upTo.includes(d.level));
    await expect(controls).toHaveCount(expected.length);
    // Cada control tiene el título, la explicación y el default del catálogo.
    for (const def of expected) {
      const c = page.locator(`[data-setting="${def.id}"]`);
      await expect(c).toContainText(title(def.id));
      await expect(c).toContainText("Por defecto:");
    }
  }
  await expectAccessible(page, "ajustes (experto)");
});

test("cambiar, ver el valor y restaurar un ajuste", async ({ page }) => {
  await open(page, "configured", "settings");
  const sat = page.getByRole("slider", { name: "Realce de colores" });
  await expect(sat).toHaveAttribute("aria-valuetext", "×1,30");
  await sat.focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await expect(sat).toHaveAttribute("aria-valuetext", "×1,40");
  // Al volver a abrir (sin reiniciar el escenario del mock), el valor sigue guardado.
  await expect.poll(() => page.evaluate(() => (window as any).__openlightMock.state.global.saturation)).toBe(1.4);
  await page.goto("/?fast=1#/settings");
  await expect(page.getByRole("slider", { name: "Realce de colores" })).toHaveAttribute("aria-valuetext", "×1,40");
  await page.getByRole("button", { name: "Restaurar Realce de colores" }).click();
  await expect(page.getByRole("slider", { name: "Realce de colores" })).toHaveAttribute("aria-valuetext", "×1,30");
});

test("ajustes de un foco heredan los globales y se pueden pisar", async ({ page }) => {
  await open(page, "configured", "settings");
  await page.getByLabel("Se aplica a").selectOption({ label: "Lámpara escritorio" });
  const own = page.locator('[data-setting="brightness_max"]');
  await expect(own).not.toContainText("Heredado");
  await expect(page.locator('[data-setting="saturation"]')).toContainText("Heredado");
});

test("perfiles: crear, renombrar, asignar atajo, activar y borrar", async ({ page }) => {
  await open(page, "configured", "settings");
  const profiles = page.getByRole("complementary", { name: "Perfiles" });
  await profiles.getByLabel("Perfil nuevo").fill("Series");
  await profiles.getByRole("button", { name: "Crear" }).click();
  await expect(page.getByLabel("Se aplica a")).toHaveValue(/profile:custom-/);

  const item = profiles.locator("li", { hasText: "Series" });
  await expect(item).toBeVisible();
  await item.getByRole("button", { name: "Renombrar" }).click();
  await expect(item.getByRole("textbox")).toHaveValue("Series");
  await item.getByRole("textbox").fill("Series de noche");
  await item.getByRole("button", { name: "Guardar" }).click();
  const renamed = profiles.locator("li", { hasText: "Series de noche" });
  await expect(renamed).toBeVisible();

  await renamed.getByRole("button", { name: /atajo de Series de noche/ }).click();
  await page.keyboard.press("Control+Alt+9");
  await expect(renamed.locator("kbd")).toHaveText("CmdOrCtrl+Alt+9");

  await renamed.getByRole("button", { name: "Activar" }).click();
  await expect(renamed.getByText("Activo")).toBeVisible();
  await profiles.locator("li", { hasText: "Películas" }).getByRole("button", { name: "Duplicar" }).click();
  await expect(profiles.locator("li", { hasText: "Copia de Películas" })).toBeVisible();

  await renamed.getByRole("button", { name: "Borrar Series de noche" }).click();
  await expect(profiles.locator("li", { hasText: "Series de noche" })).toHaveCount(0);
});

test("el brillo máximo no baja del mínimo y explica que no apaga el foco", async ({ page }) => {
  await open(page, "configured", "settings");
  const min = page.getByRole("slider", { name: "Brillo mínimo" });
  const max = page.getByRole("slider", { name: "Brillo máximo" });
  await expect(page.locator('[data-setting="brightness_max"]')).toContainText("No apaga el foco");

  // El mínimo en 5 %: "Inicio" lleva el máximo ahí, no a cero.
  await max.focus();
  await page.keyboard.press("Home");
  await expect(max).toHaveAttribute("aria-valuetext", "5 %");

  // Y con el mínimo más alto, el máximo se frena en él.
  await page.getByRole("button", { name: "Restaurar Brillo máximo" }).click();
  await expect(max).toHaveAttribute("aria-valuetext", "100 %");
  await min.fill("0.3");
  await expect(min).toHaveAttribute("aria-valuetext", "30 %");
  await max.focus();
  await page.keyboard.press("Home");
  await expect(max).toHaveAttribute("aria-valuetext", "30 %");
  await expect(page.getByRole("alert")).toHaveCount(0);
});
