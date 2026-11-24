import { expect, test } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

// El idioma se puede cambiar desde el asistente inicial, que es pantalla completa y no tiene riel.
const ES = "Tus focos, siguiendo tu pantalla";
const EN = "Your bulbs, following your screen";

test("la bienvenida muestra el selector de idioma, con cada idioma en su propio nombre", async ({ page }) => {
  await open(page, "fresh");
  const group = page.getByRole("group", { name: "Idioma / Language" });
  await expect(group).toBeVisible();
  const es = group.getByRole("button", { name: "Español" });
  const en = group.getByRole("button", { name: "English" });
  await expect(es).toHaveAttribute("aria-pressed", "true");
  await expect(en).toHaveAttribute("aria-pressed", "false");
  await expectAccessible(page, "bienvenida con selector de idioma");
});

test("cambiar de idioma en la bienvenida cambia el título, y se puede volver", async ({ page }) => {
  await open(page, "fresh");
  const title = page.locator("#step-title");
  await expect(title).toHaveText(ES);

  await page.getByRole("button", { name: "English" }).click();
  await expect(title).toHaveText(EN);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.getByRole("button", { name: "English" })).toHaveAttribute("aria-pressed", "true");
  await expectAccessible(page, "bienvenida en inglés");

  await page.getByRole("button", { name: "Español" }).click();
  await expect(title).toHaveText(ES);
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
});

test("el idioma elegido sigue al avanzar de paso y se guarda al recargar", async ({ page }) => {
  await open(page, "fresh");
  await page.getByRole("button", { name: "English" }).click();
  await page.getByTestId("next").click();
  await expect(page.locator("#step-title")).toHaveText("System permissions");
  // El selector sigue ahí en todos los pasos.
  await expect(page.getByRole("group", { name: "Idioma / Language" })).toBeVisible();

  // Recargar: el backend falso arranca de cero (Bienvenida), pero el idioma quedó guardado.
  await page.reload();
  await expect(page.locator("#step-title")).toHaveText(EN);
});

test("se cambia con el teclado", async ({ page }) => {
  await open(page, "fresh");
  const en = page.getByRole("button", { name: "English" });
  await en.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#step-title")).toHaveText(EN);
});
