// El estilo global de los selects, en los cuatro lugares donde hay uno.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { contrast, expectVisibleFocus, open } from "./helpers";

/** El asistente parado en el paso de las claves, el único con un select. */
async function credentials(page: Page) {
  await open(page, "fresh");
  await page.evaluate(() => {
    const state = JSON.parse(localStorage.getItem("openlight.mock")!);
    state.onboarding = { step: "credentials", completed: ["welcome", "permissions", "tuya_account", "cloud_project", "link_app"] };
    localStorage.setItem("openlight.mock", JSON.stringify(state));
  });
  await page.goto("/?fast=1#/onboarding");
}

const SELECTS: [string, (page: Page) => Promise<Locator>][] = [
  ["perfil del panel", async (page) => (await open(page, "configured", "dashboard"), page.getByLabel("Perfil", { exact: true }))],
  ["foco del manual", async (page) => (await open(page, "configured", "manual"), page.getByLabel("Foco", { exact: true }))],
  ["ámbito de los ajustes", async (page) => (await open(page, "configured", "settings"), page.getByLabel("Se aplica a"))],
  ["Data Center del asistente", async (page) => (await credentials(page), page.getByLabel("Data Center"))],
];

const style = (target: Locator) =>
  target.evaluate((el) => {
    const c = getComputedStyle(el);
    return { height: el.getBoundingClientRect().height, radius: c.borderTopLeftRadius, border: `${c.borderTopWidth} ${c.borderTopStyle}`, font: `${c.fontSize} ${c.fontFamily}` };
  });

for (const scheme of ["light", "dark"] as const) {
  for (const [name, find] of SELECTS) {
    test(`select ${name} (${scheme}): chevron propio, contraste y foco visible`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
      const select = await find(page);
      await expect(select).toHaveCSS("appearance", "none");
      await expect(select).toHaveCSS("background-image", /gradient/);
      await page.mouse.move(0, 0);
      expect(await contrast(select, "text")).toBeGreaterThanOrEqual(4.5);
      expect(await contrast(select, ["--chevron", "--well"])).toBeGreaterThanOrEqual(3);
      await select.hover();
      await expect.poll(() => contrast(select, ["--chevron", "--well"])).toBeGreaterThanOrEqual(3);

      // Con el teclado: se llega desde el control anterior.
      await page.mouse.move(0, 0);
      await select.focus();
      await page.keyboard.press("Shift+Tab");
      await page.keyboard.press("Tab");
      await expectVisibleFocus(select);
      await expect(select).toHaveCSS("border-top-left-radius", "18px");
    });
  }
}

test("el select tiene el mismo alto, radio, borde y tipografía que un input", async ({ page }) => {
  await credentials(page);
  // El Access ID usa la fuente de datos; el secreto a la vista, también: se compara el tamaño.
  const input = await style(page.getByRole("textbox", { name: "Access ID" }));
  const select = await style(page.getByLabel("Data Center"));
  expect({ ...select, font: select.font.split(" ")[0] }).toEqual({ ...input, font: input.font.split(" ")[0] });
  await open(page, "configured", "settings");
  const name = await style(page.getByLabel("Perfil nuevo"));
  expect(await style(page.getByLabel("Se aplica a"))).toEqual(name);
});

test("deshabilitado se ve distinto y no parece clickeable", async ({ page }) => {
  await open(page, "configured", "dashboard");
  const select = page.getByLabel("Perfil", { exact: true });
  const enabled = await select.evaluate((el) => getComputedStyle(el).color);
  await select.evaluate((el: HTMLSelectElement) => (el.disabled = true));
  await expect(select).toHaveCSS("cursor", "not-allowed");
  await expect(select).not.toHaveCSS("color", enabled);
});
