// «Control manual» como sección del riel.
import { expect, test, type Page } from "@playwright/test";
import { expectNoViolations, open } from "./helpers";

const rail = (page: Page) => page.getByRole("navigation", { name: "Secciones" });
const bulb = (page: Page) => page.getByLabel("Foco", { exact: true });
const DESK = "eb81f0c2a94d37e6b5a1c8";

/** El backend falso guarda su estado: se lo cambia y se recarga sin escenario. */
async function withoutDevices(page: Page, hash: string) {
  await open(page, "configured", "dashboard");
  await page.evaluate(() => {
    const state = JSON.parse(localStorage.getItem("openlight.mock")!);
    state.devices = [];
    localStorage.setItem("openlight.mock", JSON.stringify(state));
  });
  await page.goto(`/?fast=1#/${hash}`);
}

test("el riel muestra «Control manual» debajo de «Panel» y lleva al manual", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await expect(rail(page).getByRole("link")).toHaveText(["Panel", "Control manual", "Ajustes", "Dispositivos", "Diagnóstico", "Menú del tray"]);
  const link = rail(page).getByRole("link", { name: "Control manual" });
  await link.click();
  await expect(page).toHaveURL(/#\/manual$/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Control manual");
  await expect(link).toHaveAttribute("aria-current", "page");
  // Sin foco en la ruta abre el primero.
  await expect(bulb(page).locator("option:checked")).toHaveText("Foco living");
  await expect(page).toHaveTitle("Control manual · OpenLight");
});

test("el ítem del riel está en inglés", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  await page.getByRole("navigation", { name: "Sections" }).getByRole("link", { name: "Manual control" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Manual control");
});

test("la tarjeta del panel sigue abriendo el manual de ese foco", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await page.getByRole("article", { name: "Lámpara escritorio" }).getByRole("button", { name: "Control manual" }).click();
  await expect(page).toHaveURL(new RegExp(`#/manual/${DESK}$`));
  await expect(bulb(page).locator("option:checked")).toHaveText("Lámpara escritorio");
  await expect(rail(page).getByRole("link", { name: "Control manual" })).toHaveAttribute("aria-current", "page");
});

test("un foco que ya no existe abre el primero", async ({ page }) => {
  await open(page, "configured", "manual/no-existe");
  await expect(bulb(page).locator("option:checked")).toHaveText("Foco living");
});

test("sin focos, el manual manda a Dispositivos", async ({ page }) => {
  await withoutDevices(page, "manual");
  await expect(page.getByRole("heading", { name: "Todavía no hay focos para controlar." })).toBeVisible();
  await expect(bulb(page)).toHaveCount(0);
  await expectNoViolations(page, "control manual sin focos");
  await page.getByRole("button", { name: "Ir a Dispositivos" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos");
  await expect(page.getByRole("button", { name: "Agregar focos" })).toBeVisible();
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe en el manual (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
    await open(page, "configured", "manual");
    await expectNoViolations(page, "control manual, color");
    await page.getByRole("radio", { name: "Blanco" }).check();
    await expectNoViolations(page, "control manual, blanco");
  });
}

test("con la ventana al mínimo de alto entran todas las secciones del riel", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 520 });
  await open(page, "configured", "dashboard");
  // Como en la app real: sin la maqueta del tray, que solo existe con el backend falso.
  await page.locator('nav a[href="#/tray"]').evaluate((a) => a.parentElement!.remove());
  const nav = rail(page);
  await expect(nav.getByRole("button", { name: "EN", exact: true })).toBeInViewport({ ratio: 1 });
  expect(await nav.evaluate((el) => el.scrollHeight - el.clientHeight)).toBe(0);
});
