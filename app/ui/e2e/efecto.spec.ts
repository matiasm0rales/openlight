// Sacar un foco del efecto y usarlo aparte mientras el otro sigue la pantalla.
import { expect, test, type Page } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

const LIVING = "bf3a9c1e7d2b4f6a8c0e1d";
const mock = (page: Page, expression: string) => page.evaluate(`(() => { const m = window.__openlightMock; return ${expression}; })()`);

async function running(page: Page) {
  await open(page, "configured", "dashboard");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
}

test("control manual con el efecto andando: ofrece sacar el foco y después devolverlo", async ({ page }) => {
  await running(page);
  await page.getByRole("article", { name: "Foco living" }).getByRole("button", { name: "Control manual" }).click();
  await expect(page.getByText("Este foco está siguiendo la pantalla")).toBeVisible();
  await expect(page.getByRole("button", { name: "Azul" })).toBeDisabled();
  await expect(page.getByRole("switch", { name: "Encendido" })).toBeDisabled();
  await expectAccessible(page, "control manual con el foco en el efecto");

  await page.getByRole("button", { name: "Sacar este foco del efecto" }).click();
  await expect(page.getByText("Este foco está fuera del efecto")).toBeVisible();
  await page.getByRole("button", { name: "Azul" }).click();
  await expect.poll(() => mock(page, `m.sims.get("${LIVING}").manual?.kind`)).toBe("colour");
  // (Queda a la vista el aviso del otro foco, con la clave rechazada: no es de este.)
  await expect(page.getByRole("alert").filter({ hasText: "siguiendo la pantalla" })).toHaveCount(0);
  await expectAccessible(page, "control manual con el foco fuera del efecto");

  await page.getByRole("button", { name: "Volver a incluirlo en el efecto" }).click();
  await expect(page.getByRole("button", { name: "Azul" })).toBeDisabled();
});

test("el aviso device_in_effect ofrece sacar el foco y no habla de otras apps", async ({ page }) => {
  await running(page);
  await page.getByRole("link", { name: "Dispositivos" }).click();
  await page.locator("li.device", { hasText: "Foco living" }).getByRole("button", { name: "Identificar" }).click();
  const alert = page.getByRole("alert").filter({ hasText: "Este foco está siguiendo la pantalla" });
  await expect(alert).toContainText("device_in_effect");
  await expect(alert).not.toContainText("Smart Life");
  await expectAccessible(page, "aviso device_in_effect");
  await alert.getByRole("button", { name: "Sacar del efecto" }).click();
  await expect(alert).toHaveCount(0);
  await expect.poll(() => mock(page, `m.state.devices.find((d) => d.id === "${LIVING}").in_effect`)).toBe(false);
});

test("calibrar un foco que está en el efecto: al sacarlo, la calibración arranca", async ({ page }) => {
  await running(page);
  await page.getByRole("article", { name: "Foco living" }).getByRole("button", { name: "Calibrar" }).click();
  await page.getByRole("alert").getByRole("button", { name: "Sacar del efecto" }).click();
  await expect(page.getByRole("slider", { name: "Tono del foco" })).toBeVisible();
});
