// Capturas de las pantallas principales para el PR y la guía (Q2). No corre en `pnpm test`:
// `CAPTURAS=<carpeta> pnpm exec playwright test e2e/capturas.spec.ts`.
import { expect, test } from "@playwright/test";

const out = process.env.CAPTURAS;
test.skip(!out, "solo con CAPTURAS=<carpeta>");
test.use({ viewport: { width: 1360, height: 900 } });

for (const scheme of ["dark", "light"] as const) {
  test(`capturas ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
    const shot = async (name: string) => {
      await page.waitForTimeout(400);
      await page.screenshot({ path: `${out}/${scheme}-${name}.png` });
    };
    await page.goto("/?scenario=fresh&fast=1");
    await expect(page.locator("#step-title")).toBeVisible();
    await shot("01-asistente-bienvenida");
    await page.evaluate(() => (window as any).__openlightMock.state.onboarding.step);
    await page.goto("/?scenario=configured&fast=1#/dashboard");
    await page.getByRole("switch", { name: "Arrancar" }).click();
    await page.waitForTimeout(2500);
    await shot("02-panel");
    await page.getByRole("link", { name: "Ajustes" }).click();
    await page.getByRole("radio", { name: "Avanzado" }).check();
    await shot("03-ajustes");
    await page.getByRole("link", { name: "Diagnóstico" }).click();
    await page.waitForTimeout(3000);
    await shot("04-diagnostico");
    await page.getByRole("link", { name: "Dispositivos" }).click();
    await shot("05-dispositivos");
    await page.goto("/?fast=1#/manual");
    await page.getByRole("button", { name: "Naranja" }).click();
    await shot("06-manual");
    await page.goto("/?fast=1#/tray");
    await shot("07-tray");
    await page.goto("/?fast=1#/calibrate");
    await expect(page.getByRole("slider", { name: "Tono del foco" })).toBeVisible();
    await page.getByRole("button", { name: "Ir a Azul" }).click();
    await shot("08-calibracion");
  });
}

test("capturas del asistente", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await page.goto("/?scenario=fresh&fast=1");
  const next = page.getByTestId("next");
  await next.click();
  await page.getByRole("button", { name: "Abrir Grabación de pantalla" }).click();
  await expect(page.getByText("Concedido")).toHaveCount(2);
  await next.click();
  await page.getByLabel("Ya tengo cuenta en iot.tuya.com").check();
  await next.click();
  await page.getByLabel("Argentina, Uruguay, Chile, México y el resto de América").check();
  await page.getByLabel("Ya creé el proyecto").check();
  await page.screenshot({ path: `${out}/dark-09-asistente-proyecto.png` });
  await next.click();
  await page.getByLabel("Ya vinculé mi cuenta de Smart Life").check();
  await next.click();
  await page.getByRole("textbox", { name: "Access ID" }).fill("p9xk2wq7vd4mfa1ztc8e");
  await page.getByLabel("Access Secret", { exact: true }).fill("0123456789abcdef0123456789abcdef");
  await page.getByLabel("Data Center").selectOption("eu");
  await page.getByRole("button", { name: "Probar", exact: true }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await page.screenshot({ path: `${out}/dark-10-asistente-region.png` });
  await page.getByRole("button", { name: "Usar Western America" }).click();
  await next.click();
  await expect(page.getByText("encontrados en tu red")).toBeVisible();
  await page.screenshot({ path: `${out}/dark-11-asistente-focos.png` });
  await next.click();
  for (const name of ["Foco living", "Lámpara escritorio"]) {
    const card = page.locator("li", { hasText: name });
    await card.getByRole("button", { name: "Probar rojo, verde y azul" }).click();
    await card.getByRole("button", { name: "Sí, los vi" }).click();
  }
  await next.click();
  await page.waitForTimeout(800);
  await page.screenshot({ path: `${out}/dark-12-asistente-zonas.png` });
});
