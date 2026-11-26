import { expect, test, type Page } from "@playwright/test";
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

/** Los valores propios que el foco falso tiene guardados. */
const deskOverrides = (page: Page) =>
  page.evaluate(() => {
    const { state } = (window as any).__openlightMock;
    return state.deviceOverrides[state.devices.find((d: { name: string }) => d.name === "Lámpara escritorio").id];
  });
const card = (page: Page, id: string) => page.locator(`[data-setting="${id}"]`);

test("en «Todos los focos» se ve qué foco tiene un valor propio y se puede usar el de todos", async ({ page }) => {
  await open(page, "configured", "settings");
  const max = card(page, "brightness_max");
  await expect(max).toContainText("Lámpara escritorio usa su propio valor (70 %): lo que cambies acá no le llega.");
  await expect(card(page, "saturation")).not.toContainText("usa su propio valor");
  await expectAccessible(page, "ajustes globales con un foco con valor propio");

  // El de todos se mueve y el del foco queda.
  const slider = page.getByRole("slider", { name: "Brillo máximo" });
  await slider.fill("0.8");
  await expect.poll(() => page.evaluate(() => (window as any).__openlightMock.state.global.brightness_max)).toBe(0.8);
  await expect(max).toContainText("usa su propio valor (70 %)");

  await max.getByRole("button", { name: "Usar el valor de todos en Lámpara escritorio: Brillo máximo" }).click();
  await expect(max).not.toContainText("usa su propio valor");
  await expect.poll(() => deskOverrides(page)).toEqual({});
  await expect(slider).toBeFocused();
  await expect(slider).toHaveAttribute("aria-valuetext", "80 %");
  // Y en el ámbito del foco ya figura como el de todos.
  await page.getByLabel("Se aplica a").selectOption({ label: "Lámpara escritorio" });
  await expect(max).toContainText("De todos los focos");
  await expect(page.getByRole("slider", { name: "Brillo máximo" })).toHaveAttribute("aria-valuetext", "80 %");
});

test("en un foco se distingue lo propio de lo que viene de todos", async ({ page }) => {
  await open(page, "configured", "settings");
  await page.getByLabel("Se aplica a").selectOption({ label: "Lámpara escritorio" });
  const max = card(page, "brightness_max");
  const sat = card(page, "saturation");
  await expect(max.getByText("Valor propio", { exact: true })).toBeVisible();
  await expect(max).toContainText("Sin él usaría 100 %, de todos los focos.");
  await expect(sat.getByText("De todos los focos", { exact: true })).toBeVisible();
  await expect(sat.getByRole("button")).toHaveCount(0);
  await expectAccessible(page, "ajustes de un foco");

  // Moverlo lo vuelve propio.
  await page.getByRole("slider", { name: "Realce de colores" }).fill("2");
  await expect(sat.getByText("Valor propio", { exact: true })).toBeVisible();
  await expect(sat).toContainText("Sin él usaría ×1,30, de todos los focos.");

  await max.getByRole("button", { name: "Usar el valor de todos: Brillo máximo" }).click();
  await expect(max.getByText("De todos los focos", { exact: true })).toBeVisible();
  await expect(page.getByRole("slider", { name: "Brillo máximo" })).toHaveAttribute("aria-valuetext", "100 %");
  await expect(page.getByRole("slider", { name: "Brillo máximo" })).toBeFocused();
  await expect.poll(() => deskOverrides(page)).toEqual({ saturation: 2 });
});

test("en un foco, lo que pone el perfil activo se atribuye al perfil", async ({ page }) => {
  await open(page, "configured", "settings");
  await page.getByRole("complementary", { name: "Perfiles" }).locator("li", { hasText: "Películas" }).getByRole("button", { name: "Activar" }).click();
  await page.getByLabel("Se aplica a").selectOption({ label: "Lámpara escritorio" });
  await expect(card(page, "saturation").getByText("Del perfil «Películas»", { exact: true })).toBeVisible();
  await expect(card(page, "brightness_max")).toContainText("Sin él usaría 80 %, del perfil «Películas».");
});

test("valores propios en inglés", async ({ page }) => {
  await open(page, "configured", "settings");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  const max = card(page, "brightness_max");
  await expect(max).toContainText("Lámpara escritorio uses its own value (70 %), so changes here don't reach it.");
  await expect(max.getByRole("button", { name: "Use the shared value on Lámpara escritorio: Maximum brightness" })).toHaveText("Use the shared value");
  await page.getByLabel("Applies to").selectOption({ label: "Lámpara escritorio" });
  await expect(max.getByText("Own value", { exact: true })).toBeVisible();
  await expect(max).toContainText("Without it, it would use 100 %, from all bulbs.");
  await expect(card(page, "saturation").getByText("From all bulbs", { exact: true })).toBeVisible();
});

test("perfiles: crear, renombrar, asignar atajo, activar y eliminar", async ({ page }) => {
  await open(page, "configured", "settings");
  const profiles = page.getByRole("complementary", { name: "Perfiles" });
  await profiles.getByLabel("Perfil nuevo").fill("Series");
  await profiles.getByRole("button", { name: "Crear" }).click();
  await expect(page.getByLabel("Se aplica a")).toHaveValue(/profile:custom-/);

  const item = profiles.locator("li", { hasText: "Series" });
  await expect(item).toBeVisible();
  await item.getByRole("button", { name: "Más acciones de Series" }).click();
  await page.getByRole("menuitem", { name: "Renombrar" }).click();
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

  await renamed.getByRole("button", { name: "Eliminar el perfil Series de noche" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Eliminar perfil" }).click();
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

test("con un perfil activo, el ajuste global se puede mover y avisa quién lo tapa", async ({ page }) => {
  await open(page, "configured", "settings");
  await page.getByRole("complementary", { name: "Perfiles" }).locator("li", { hasText: "Películas" }).getByRole("button", { name: "Activar" }).click();
  const control = page.locator('[data-setting="saturation"]');
  await expect(control).toContainText("El perfil «Películas» usa su propio valor mientras está activo");
  const sat = page.getByRole("slider", { name: "Realce de colores" });
  await expect(sat).toHaveAttribute("aria-valuetext", "×1,30");
  await sat.fill("2");
  await expect.poll(() => page.evaluate(() => (window as any).__openlightMock.state.global.saturation)).toBe(2);
  await expect(sat).toHaveAttribute("aria-valuetext", "×2,00");
  // Y el brillo que la lámpara tiene propio también se avisa.
  await expect(page.locator('[data-setting="brightness_max"]')).toContainText("Lámpara escritorio usa su propio valor (70 %)");
  // En el perfil activo, también.
  await page.getByLabel("Se aplica a").selectOption({ label: "Películas" });
  await expect(page.locator('[data-setting="brightness_max"]')).toContainText("Tienen su propio valor: Lámpara escritorio");
  await expectAccessible(page, "ajustes con avisos de valor tapado");
});
