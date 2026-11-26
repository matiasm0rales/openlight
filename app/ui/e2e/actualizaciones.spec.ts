import { expect, test, type Page } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

// Las actualizaciones contra el updater falso (`?update=<escenario>`, src/lib/mocks/updater.ts).
const RELEASES = "https://github.com/matiasm0rales/openlight/releases/latest";

const calls = (page: Page): Promise<string[]> => page.evaluate(() => (window as any).__openlightMock.updater.calls);
const restarted = (page: Page): Promise<boolean> => page.evaluate(() => (window as any).__openlightMock.updater.restarted);
const scenario = (page: Page, value: string) => page.evaluate((v) => ((window as any).__openlightMock.updater.scenario = v), value);
const notice = (page: Page) => page.getByRole("region", { name: /Hay una versión nueva/ });
const install = (page: Page) => page.getByRole("button", { name: "Actualizar y reiniciar" });

test("sin versión nueva el panel no muestra nada", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await expect.poll(() => calls(page)).toContain("check");
  await expect(page.getByText(/Hay una versión nueva/)).toHaveCount(0);
});

test("el panel avisa la versión nueva con sus notas, sin tapar nada", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=available");
  await expect(notice(page)).toBeVisible();
  await expect(notice(page).getByRole("heading")).toHaveText("Hay una versión nueva: 0.2.0");
  await expect(notice(page)).toContainText("Tenés la 0.1.0");
  const notes = notice(page).getByRole("region", { name: "Novedades" });
  await expect(notes).toContainText("• Escenas en el control manual.");
  await expect(notes).not.toContainText("**");
  // No es un diálogo: el panel sigue usable.
  await expect(page.getByRole("switch", { name: "Arrancar" })).toBeEnabled();
  await expectAccessible(page, "panel con versión nueva");
});

test("actualizar muestra el progreso, devuelve los focos antes de instalar y reinicia", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=hang");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  await install(page).click();

  const bar = page.getByRole("progressbar", { name: "Descarga de la actualización" });
  await expect(bar).toBeVisible();
  await expect(notice(page).getByRole("status")).toHaveText("Descargando la actualización…");
  await expect(notice(page)).toContainText("40 %");
  await expect(bar).toHaveAttribute("value", "4800000");
  await expect(bar).toHaveAttribute("max", "12000000");
  await expect(install(page)).toHaveCount(0);
  await expectAccessible(page, "descargando la actualización");
  // El efecto sigue andando mientras descarga.
  expect(await calls(page)).not.toContain("update_prepare");

  await page.evaluate(() => (window as any).__openlightMock.updater.finishDownload());
  const installed = page.getByRole("region", { name: "La versión 0.2.0 ya está instalada" });
  await expect(installed.getByRole("status")).toHaveText("Reiniciando OpenLight…");
  await expect.poll(() => restarted(page)).toBe(true);
  expect((await calls(page)).slice(-4)).toEqual(["download", "update_prepare", "install", "restart"]);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Todo en pausa");
});

test("sin red en medio de la descarga: lo dice y se puede reintentar", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=offline_download");
  await install(page).click();
  const alert = notice(page).getByRole("alert");
  await expect(alert).toContainText("No hay conexión");
  await expect(alert).toContainText("Revisá que la computadora tenga internet");
  await expectAccessible(page, "actualización sin red");
  await scenario(page, "available");
  await alert.getByRole("button", { name: "Reintentar" }).click();
  await expect.poll(() => restarted(page)).toBe(true);
});

test("firma inválida: no instala nada y ofrece bajarla de la página oficial", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=bad_signature");
  await install(page).click();
  const alert = notice(page).getByRole("alert");
  await expect(alert).toContainText("La descarga no pasó la verificación de firma");
  await expect(alert).toContainText("No se instaló nada");
  await expect(alert.getByRole("button", { name: "Reintentar" })).toBeVisible();
  expect(await calls(page)).not.toContain("install");
  await page.evaluate(() => (window.open = ((url: string) => ((window as any).__opened = url, null)) as typeof window.open));
  await alert.getByRole("button", { name: "Descargar desde GitHub" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__opened)).toBe(RELEASES);
  await expectAccessible(page, "actualización con firma inválida");
});

test("si instalar falla, el efecto vuelve a arrancar", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=install_fails");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  await install(page).click();
  await expect(notice(page).getByRole("alert")).toContainText("No se pudo reemplazar la app");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  expect((await calls(page)).slice(-3)).toEqual(["install", "update_abort", "session_start"]);
});

test("si no se puede reiniciar, queda instalada con 'Reiniciar ahora'", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=restart_fails");
  await install(page).click();
  const card = page.getByRole("region", { name: "La versión 0.2.0 ya está instalada" });
  await expect(card.getByRole("alert")).toContainText("No se pudo reiniciar");
  await scenario(page, "available");
  await card.getByRole("button", { name: "Reiniciar ahora" }).click();
  await expect.poll(() => restarted(page)).toBe(true);
});

test("'Más tarde' saca el aviso del panel; en Ajustes sigue disponible", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=available");
  await notice(page).getByRole("button", { name: "Más tarde" }).click();
  await expect(notice(page)).toHaveCount(0);
  await page.getByRole("link", { name: "Ajustes" }).click();
  await expect(notice(page)).toBeVisible();
  await expect(install(page)).toBeVisible();
  await expectAccessible(page, "ajustes con versión nueva");
});

test("Ajustes: versión actual y búsqueda a mano", async ({ page }) => {
  await open(page, "configured", "settings");
  const panel = page.getByRole("region", { name: "Actualizaciones" });
  await expect(panel).toContainText("Versión actual: 0.1.0");
  await panel.getByRole("button", { name: "Buscar actualizaciones" }).click();
  await expect(panel.getByRole("status")).toHaveText("Ya tenés la última versión.");

  await scenario(page, "offline");
  await panel.getByRole("button", { name: "Buscar actualizaciones" }).click();
  await expect(panel.getByRole("alert")).toContainText("No hay conexión");
  await expectAccessible(page, "ajustes sin red al buscar");

  await scenario(page, "available");
  await panel.getByRole("alert").getByRole("button", { name: "Reintentar" }).click();
  await expect(panel.getByRole("alert")).toHaveCount(0);
  await expect(notice(page).getByRole("heading", { level: 3 })).toHaveText("Hay una versión nueva: 0.2.0");
});

test("sin versiones publicadas está al día, y la ventana que reaparece no repite la búsqueda", async ({ page }) => {
  await open(page, "configured", "settings", "update=no_release");
  const panel = page.getByRole("region", { name: "Actualizaciones" });
  await expect(panel.getByRole("status")).toHaveText("Ya tenés la última versión.");
  // macOS puede volver a mostrar la ventana muchas veces seguidas.
  await page.evaluate(async () => {
    for (let i = 0; i < 20; i++) {
      document.dispatchEvent(new Event("visibilitychange"));
      await new Promise((r) => setTimeout(r, 60));
    }
  });
  expect(await calls(page)).toEqual(["check"]);
  await panel.getByRole("button", { name: "Buscar actualizaciones" }).click();
  await expect.poll(() => calls(page)).toEqual(["check", "check"]);
  await expect(panel.getByRole("status")).toHaveText("Ya tenés la última versión.");
  await expect(panel.getByRole("alert")).toHaveCount(0);
});

test("la búsqueda automática se puede apagar y se recuerda", async ({ page }) => {
  await open(page, "configured", "settings", "update=available");
  const auto = page.getByRole("switch", { name: "Buscar actualizaciones automáticamente" });
  await expect(auto).toHaveAttribute("aria-checked", "true");
  await auto.click();
  await expect(auto).toHaveAttribute("aria-checked", "false");
  await page.reload();
  await expect(auto).toHaveAttribute("aria-checked", "false");
  await expect(page.getByRole("region", { name: "Actualizaciones" })).toContainText("Versión actual: 0.1.0");
  expect(await calls(page)).not.toContain("check");
});

test("la ventana de calibración no busca actualizaciones", async ({ page }) => {
  await open(page, "configured", "calibrate", "update=available");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  expect(await calls(page)).not.toContain("check");
});

test("en inglés", async ({ page }) => {
  await open(page, "configured", "dashboard", "update=bad_signature");
  await page.getByRole("button", { name: "EN", exact: true }).click();
  const card = page.getByRole("region", { name: "A new version is available: 0.2.0" });
  await card.getByRole("button", { name: "Update and restart" }).click();
  await expect(card.getByRole("alert")).toContainText("The download failed the signature check");
});
