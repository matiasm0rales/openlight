import { expect, test } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

test("asistente de punta a punta: de cero a los focos siguiendo la pantalla", async ({ page }) => {
  await open(page, "fresh");
  await expect(page).toHaveURL(/#\/onboarding/);
  const next = page.getByTestId("next");
  const title = page.locator("#step-title");

  // 1. Bienvenida
  await expect(title).toHaveText("Tus focos, siguiendo tu pantalla");
  await expectAccessible(page, "bienvenida");
  await next.click();

  // 2. Permisos: Grabación de pantalla sin pedir → "Siguiente" no avanza hasta concederlo.
  await expect(title).toHaveText("Permisos del sistema");
  await expect(page.getByText("Sin pedir")).toBeVisible();
  await expect(next).toHaveAttribute("aria-disabled", "true");
  await expect(page.getByText("Falta conceder los permisos de arriba.")).toBeVisible();
  await page.getByRole("button", { name: "Abrir Grabación de pantalla" }).click();
  await expect(page.getByText("Concedido")).toHaveCount(2, { timeout: 5000 });
  await expectAccessible(page, "permisos");
  await next.click();

  // 3–5. Guía del portal (contenido editable) + confirmaciones.
  await expect(title).toHaveText("Creá tu cuenta en Tuya Developer");
  await expect(page.getByText("Abrí iot.tuya.com y creá una cuenta gratis")).toBeVisible();
  await page.getByLabel("Ya tengo cuenta en iot.tuya.com").check();
  await expectAccessible(page, "cuenta tuya");
  await next.click();

  await expect(title).toHaveText("Creá el proyecto Cloud");
  await page.getByLabel("Argentina, Uruguay, Chile, México y el resto de América").check();
  await expect(page.getByRole("status").filter({ hasText: "Western America" })).toBeVisible();
  await page.getByLabel("Ya creé el proyecto").check();
  await next.click();

  await expect(title).toHaveText("Vinculá Smart Life al proyecto");
  await page.getByLabel("Ya vinculé mi cuenta de Smart Life").check();
  await next.click();

  // 6. Credenciales: validación de formato, Data Center equivocado y arreglo en un clic.
  await expect(title).toHaveText("Pegá el Access ID y el Access Secret");
  await page.getByRole("textbox", { name: "Access ID" }).fill("p9xk2wq7vd4mfa1ztc8e");
  await page.getByLabel("Access Secret", { exact: true }).fill("corto");
  await expect(page.getByText("El Access Secret tiene 32 caracteres y pegaste 5.")).toBeVisible();
  await page.getByLabel("Access Secret", { exact: true }).fill("0123456789abcdef0123456789abcdef");
  await page.getByLabel("Data Center").selectOption("eu");
  await page.getByRole("button", { name: "Probar", exact: true }).click();
  const alert = page.getByRole("alert").filter({ hasText: "Data Center equivocado" });
  await expect(alert).toContainText("Tus credenciales funcionan en Western America");
  await expectAccessible(page, "credenciales con error");
  await alert.getByRole("button", { name: "Usar Western America" }).click();
  await expect(page.getByText("¡Anda! Conectado con Tuya en Western America.")).toBeVisible();
  await next.click();

  // 7. Elegir focos: cruzados con la red local; el que no tiene color no se puede elegir.
  await expect(title).toHaveText("Elegí los focos");
  await expect(page.getByText("encontrados en tu red")).toBeVisible();
  await expect(page.getByRole("checkbox", { name: /Foco pasillo/ })).toBeDisabled();
  await expect(page.getByRole("checkbox", { name: /Foco living/ })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: /Lámpara escritorio/ })).toBeChecked();
  await expectAccessible(page, "elegir focos");
  await next.click();

  // 8. Probar cada foco.
  await expect(title).toHaveText("Probá cada foco");
  for (const name of ["Foco living", "Lámpara escritorio"]) {
    const card = page.locator("li", { hasText: name });
    await card.getByRole("button", { name: "Probar rojo, verde y azul" }).click();
    await card.getByRole("button", { name: "Sí, los vi" }).click();
    await expect(card.getByText("Funciona")).toBeVisible();
  }
  await next.click();

  // 9. Monitor y zonas: con dos focos arrancan en izquierda y derecha; se cambia con atajos o teclado.
  await expect(title).toHaveText("Monitor y zonas");
  const zone = page.getByRole("slider", { name: "Zona de Foco living" });
  await expect(zone).toBeVisible();
  await page.getByRole("button", { name: "Arriba", exact: true }).click();
  await zone.focus();
  await page.keyboard.press("ArrowDown");
  await expect(zone).toHaveAttribute("aria-valuetext", /desde 0%, 5%/);
  await expectAccessible(page, "zonas");
  await next.click();

  // 10. Calibrar (opcional) → 11. Listo.
  await expect(title).toHaveText("Calibrá los colores (recomendado)");
  await next.click();
  await expect(title).toHaveText("¡Listo!");
  await page.getByRole("switch", { name: "Abrir OpenLight al iniciar sesión" }).click();
  await expect(page.getByRole("switch", { name: "Abrir OpenLight al iniciar sesión" })).toHaveAttribute("aria-checked", "true");
  await page.getByTestId("start").click();

  await expect(page).toHaveURL(/#\/dashboard/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  await expect(page.getByRole("article", { name: "Foco living" }).getByText("Conectado", { exact: true })).toBeVisible();
});

test("el progreso del asistente persiste: se puede pausar y retomar", async ({ page }) => {
  await open(page, "fresh");
  await page.getByTestId("next").click();
  await page.getByRole("button", { name: "Abrir Grabación de pantalla" }).click();
  await expect(page.getByText("Concedido")).toHaveCount(2);
  await page.getByTestId("next").click();
  await expect(page.locator("#step-title")).toHaveText("Creá tu cuenta en Tuya Developer");

  // Pausar y volver: sigue en el mismo paso, con los anteriores marcados como hechos.
  await page.getByRole("button", { name: "Seguir después" }).click();
  await expect(page).toHaveURL(/#\/dashboard/);
  await expect(page.getByText("Te faltan 9 pasos del asistente.")).toBeVisible();
  await page.getByRole("button", { name: "Retomar" }).click();
  await expect(page.locator("#step-title")).toHaveText("Creá tu cuenta en Tuya Developer");

  // Al reabrir la app, el asistente retoma solo en el mismo paso.
  await page.goto("/?fast=1");
  await expect(page).toHaveURL(/#\/onboarding/);
  await expect(page.locator("#step-title")).toHaveText("Creá tu cuenta en Tuya Developer");
  await expect(page.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "3");

  // Volver atrás también funciona.
  await page.getByRole("button", { name: "Atrás" }).click();
  await expect(page.locator("#step-title")).toHaveText("Permisos del sistema");
});

test("importar el setup del script salta al paso de zonas", async ({ page }) => {
  await open(page, "fresh");
  await page.getByRole("button", { name: "Ya tengo devices.json" }).click();
  await page.getByLabel("Carpeta del script").fill("/tmp/cualquiera");
  await page.getByRole("button", { name: "Importar" }).click();
  await expect(page.getByRole("alert")).toContainText("No pude importar esa carpeta");
  await page.getByLabel("Carpeta del script").fill("~/Documents/Open RGB");
  await page.getByRole("button", { name: "Importar" }).click();
  await expect(page.getByText("Listo: 2 focos y 1 calibraciones importadas.")).toBeVisible();
  await page.getByRole("button", { name: "Seguir con las zonas" }).click();
  await expect(page.locator("#step-title")).toHaveText("Monitor y zonas");
});
