// Jerarquía de las acciones de un perfil: primaria, secundarias, menú «Más» y eliminar.
import { expect, test, type Page } from "@playwright/test";
import { contrast, expectNoViolations, expectVisibleFocus, open } from "./helpers";

const panel = (page: Page) => page.getByRole("complementary", { name: "Perfiles" });
const profile = (page: Page, id: string) => panel(page).locator(`li[data-profile="${id}"]`);

/** Ajustes con un perfil propio, «Series», al final de la lista. */
async function withCustom(page: Page) {
  await open(page, "configured", "settings");
  await panel(page).getByLabel("Perfil nuevo").fill("Series");
  await panel(page).getByRole("button", { name: "Crear" }).click();
  const item = panel(page).locator("li", { hasText: "Series" });
  await expect(item).toBeVisible();
  return {
    item,
    activate: item.getByRole("button", { name: /^(Activar|Desactivar)$/ }),
    edit: item.getByRole("button", { name: "Editar" }),
    more: item.getByRole("button", { name: "Más acciones de Series" }),
    remove: item.getByRole("button", { name: "Eliminar el perfil Series" }),
  };
}

test("activar es primaria, el resto secundarias y eliminar solo está en los propios", async ({ page }) => {
  const p = await withCustom(page);
  await expect(p.activate).toHaveClass(/\bprimary\b/);
  for (const b of [p.edit, p.more]) await expect(b).toHaveClass(/\bsecondary\b/);
  await expect(p.remove).toHaveClass(/\bdestructive\b/);
  await expect(p.remove).toHaveAttribute("title", "Eliminar");

  const movies = profile(page, "movies");
  await expect(movies.getByRole("group", { name: "Acciones de Películas" }).getByRole("button")).toHaveText(["Activar", "Editar", "Duplicar"]);
  await expect(movies.getByRole("button", { name: "Duplicar" })).toHaveClass(/\bsecondary\b/);
  // Cada acción tiene su ícono.
  for (const b of await p.item.getByRole("group").getByRole("button").all()) await expect(b.locator("svg")).toHaveCount(1);
});

test("el activo lleva distintivo y su botón pasa a secundario sin perder el foco", async ({ page }) => {
  const p = await withCustom(page);
  await p.activate.focus();
  await page.keyboard.press("Enter");
  await expect(p.item.getByText("Activo", { exact: true })).toBeVisible();
  await expect(p.activate).toHaveText("Desactivar");
  await expect(p.activate).toHaveClass(/\bsecondary\b/);
  await expect(p.activate).toBeFocused();
  await expect(panel(page).getByText("Activo", { exact: true })).toHaveCount(1);

  await page.keyboard.press("Enter");
  await expect(panel(page).getByText("Activo", { exact: true })).toHaveCount(0);
  await expect(p.activate).toHaveClass(/\bprimary\b/);
});

test("«Editar» lleva el foco al selector de ámbito, con el perfil elegido", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 680 });
  await open(page, "configured", "settings");
  await profile(page, "movies").getByRole("button", { name: "Editar" }).click();
  const scope = page.getByLabel("Se aplica a");
  await expect(scope).toHaveValue("profile:movies");
  await expect(scope).toBeFocused();
  await expect(scope).toBeInViewport();
});

test("el menú «Más» se maneja con el teclado", async ({ page }) => {
  const p = await withCustom(page);
  const menu = page.getByRole("menu", { name: "Más acciones de Series" });
  const duplicate = menu.getByRole("menuitem", { name: "Duplicar" });
  const rename = menu.getByRole("menuitem", { name: "Renombrar" });
  await expect(p.more).toHaveAttribute("aria-expanded", "false");

  await p.more.focus();
  await page.keyboard.press("Enter");
  await expect(p.more).toHaveAttribute("aria-expanded", "true");
  await expectVisibleFocus(duplicate);
  await page.keyboard.press("ArrowDown");
  await expect(rename).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(duplicate).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(rename).toBeFocused();
  await page.keyboard.press("Home");
  await expect(duplicate).toBeFocused();
  await page.keyboard.press("End");
  await expect(rename).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(p.more).toBeFocused();

  await page.keyboard.press("ArrowUp");
  await expect(rename).toBeFocused();
  // Tab cierra y sigue con lo que viene después del botón.
  await page.keyboard.press("Tab");
  await expect(menu).toHaveCount(0);
  await expect(p.remove).toBeFocused();

  // Un clic afuera también cierra, y otro en el botón abre y cierra.
  await p.more.click();
  await expect(menu).toBeVisible();
  await page.getByRole("heading", { level: 1 }).click();
  await expect(menu).toHaveCount(0);
  await p.more.click();
  await p.more.click();
  await expect(menu).toHaveCount(0);
});

test("desde «Más»: renombrar (con Escape se cancela) y duplicar", async ({ page }) => {
  const p = await withCustom(page);
  await p.more.click();
  await page.getByRole("menuitem", { name: "Renombrar" }).click();
  const name = p.item.getByRole("textbox", { name: "Renombrar" });
  await expect(name).toBeFocused();
  await expect(name).toHaveValue("Series");
  await page.keyboard.type("Otro nombre");
  await page.keyboard.press("Escape");
  await expect(name).toHaveCount(0);
  await expect(p.more).toBeFocused();

  await p.more.click();
  await page.getByRole("menuitem", { name: "Duplicar" }).click();
  await expect(panel(page).locator("li", { hasText: "Copia de Series" })).toBeVisible();
});

test("eliminar pide confirmación", async ({ page }) => {
  const p = await withCustom(page);
  const dialog = page.getByRole("dialog", { name: "¿Eliminar el perfil «Series»?" });

  await p.remove.click();
  await expect(dialog).toContainText("Esto no se puede deshacer");
  await dialog.getByRole("button", { name: "Cancelar" }).click();
  await expect(dialog).toBeHidden();
  await expect(p.item).toBeVisible();

  await p.remove.focus();
  await page.keyboard.press("Enter");
  await expect(dialog.getByRole("button", { name: "Cancelar" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(p.remove).toBeFocused();

  await page.keyboard.press("Enter");
  await dialog.getByRole("button", { name: "Eliminar perfil" }).click();
  await expect(p.item).toHaveCount(0);
  await expect(panel(page).getByRole("heading", { name: "Perfiles" })).toBeFocused();
});

for (const scheme of ["light", "dark"] as const) {
  test(`axe en Ajustes con las acciones de perfil (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
    const p = await withCustom(page);
    await p.activate.click();
    await expect(p.item.getByText("Activo", { exact: true })).toBeVisible();
    await expectNoViolations(page, "ajustes con un perfil propio activo");
    await p.more.click();
    await expectNoViolations(page, "menú «Más» abierto");
    await page.getByRole("menuitem", { name: "Renombrar" }).click();
    await expectNoViolations(page, "renombrando un perfil");
    await page.keyboard.press("Escape");
    await p.remove.click();
    await expectNoViolations(page, "confirmación para eliminar");
  });

  test(`contraste y foco de teclado de los botones de perfil (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
    const p = await withCustom(page);
    const shortcut = p.item.getByRole("button", { name: /atajo de Series/ });

    // En orden, con Tab desde el título de la lista hasta la última tarjeta.
    await shortcut.focus();
    await page.keyboard.press("Shift+Tab");
    for (const [i, button] of [p.remove, p.more, p.edit, p.activate].entries()) {
      if (i) await page.keyboard.press("Shift+Tab");
      await expectVisibleFocus(button);
    }

    const duplicate = profile(page, "movies").getByRole("button", { name: "Duplicar" });
    for (const button of [p.activate, p.edit, p.more, duplicate, shortcut]) {
      expect(await contrast(button, "text")).toBeGreaterThanOrEqual(4.5);
      await button.hover();
      await expect.poll(() => contrast(button, "text")).toBeGreaterThanOrEqual(4.5);
    }
    // Eliminar es solo un ícono: alcanza con 3:1, en reposo y lleno.
    expect(await contrast(p.remove, "text")).toBeGreaterThanOrEqual(3);
    await p.remove.hover();
    await expect.poll(() => contrast(p.remove, "text")).toBeGreaterThanOrEqual(3);
    await p.remove.click();
    const confirm = page.getByRole("dialog").getByRole("button", { name: "Eliminar perfil" });
    expect(await contrast(confirm, "text")).toBeGreaterThanOrEqual(4.5);
    await confirm.hover();
    await expect.poll(() => contrast(confirm, "text")).toBeGreaterThanOrEqual(4.5);
  });
}
