// El respaldo sin claves en Dispositivos, contra el backend falso.
import { expect, test, type Page } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

// Ids inventados, los mismos del backend falso.
const LIVING = "bf3a9c1e7d2b4f6a8c0e1d";
const STRIP = "bf5e6a7c8d9e0f1a2b3c4d";

const device = (id: string, name: string, order: number) => ({ id, name, version: "3.5", capabilities: null, zones: [], overrides: {}, calibration: null, order });

/** Un respaldo con un foco que ya está (living) y uno nuevo (la tira). */
const BACKUP = {
  format: "openlight-backup",
  version: 1,
  created_at: "2026-11-24T18:30:05Z",
  app_version: "0.1.0",
  settings: { global: {}, active_profile: null },
  profiles: [],
  // En su compu la tira iba primero (manda `order`, no el lugar en el archivo).
  devices: [device(LIVING, "Living del respaldo", 1), device(STRIP, "Tira LED tele", 0)],
  excluded_devices: [],
};

/** Toca "Importar respaldo" y elige un archivo con ese contenido. */
async function pick(page: Page, content: unknown) {
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Importar respaldo" }).click();
  const text = typeof content === "string" ? content : JSON.stringify(content);
  await (await chooser).setFiles({ name: "openlight-backup.json", mimeType: "application/json", buffer: Buffer.from(text) });
}

const backup = (page: Page) => page.getByRole("region", { name: "Respaldo" });

/** Los nombres de las tarjetas, en el orden en que se ven. */
const listed = (page: Page) => page.locator(".device h2");

test("exportar: dice dónde quedó y que no lleva claves", async ({ page }) => {
  await open(page, "configured", "devices");
  await expect(backup(page)).toContainText("No incluye las claves de los focos");
  await expect(backup(page)).toContainText("se vuelven a bajar de la nube de Tuya");
  await expectAccessible(page, "dispositivos con respaldo");
  await page.getByRole("button", { name: "Exportar respaldo" }).click();
  await expect(backup(page).getByRole("status")).toContainText(/Respaldo guardado en .*openlight-backup-\d{4}-\d{2}-\d{2}\.json/);
});

test("importar: resumen antes de aplicar, saltear y bajar las claves que faltan", async ({ page }) => {
  await open(page, "configured", "devices");
  await pick(page, BACKUP);

  const dialog = page.getByRole("dialog", { name: "Importar respaldo" });
  await expect(dialog).toContainText("Focos nuevos: 1");
  await expect(dialog).toContainText("Ya están en esta compu: 1");
  await expect(dialog).toContainText("Tira LED tele");
  await expect(dialog).toContainText("OpenLight 0.1.0");
  await expect(dialog).toContainText("quedan sin clave");
  await expect(dialog.getByRole("radio", { name: /^Saltear/ })).toBeChecked();
  await expectAccessible(page, "resumen del respaldo");
  // Mirar el resumen no cambia nada
  await expect(page.getByRole("heading", { name: "Tira LED tele" })).toHaveCount(0);

  await dialog.getByRole("button", { name: "Importar", exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(backup(page)).toContainText("Respaldo importado. Focos: 1.");
  await expect(page.getByRole("heading", { name: "Tira LED tele" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Living del respaldo" })).toHaveCount(0);
  // Saltear no reordena: lo nuevo va al final
  await expect(listed(page)).toHaveText(["Foco living", "Lámpara escritorio", "Tira LED tele"]);

  const strip = page.getByRole("listitem").filter({ has: page.getByRole("heading", { name: "Tira LED tele" }) });
  await expect(strip.getByText("Sin clave")).toBeVisible();
  await expect(backup(page)).toContainText("hasta bajar su clave");
  await expectAccessible(page, "claves pendientes");
  await backup(page).getByRole("button", { name: "Bajar claves de la nube" }).click();
  await expect(backup(page)).toContainText("Claves bajadas: 1.");
  await expect(strip.getByText("Sin clave")).toHaveCount(0);
  await expect(backup(page).getByRole("button", { name: "Bajar claves de la nube" })).toHaveCount(0);
});

test("importar pisando: los repetidos quedan como en el respaldo", async ({ page }) => {
  await open(page, "configured", "devices");
  await pick(page, BACKUP);
  const dialog = page.getByRole("dialog", { name: "Importar respaldo" });
  await expect(dialog.getByRole("radio", { name: /^Pisar/ })).toHaveAccessibleName(/su orden/);
  await dialog.getByRole("radio", { name: /^Pisar/ }).check();
  await dialog.getByRole("button", { name: "Importar", exact: true }).click();
  await expect(backup(page)).toContainText("Respaldo importado. Focos: 2.");
  // Primero los del respaldo, en su orden; después lo que solo está en esta compu
  await expect(listed(page)).toHaveText(["Tira LED tele", "Living del respaldo", "Lámpara escritorio"]);
});

test("cancelar el resumen no importa nada", async ({ page }) => {
  await open(page, "configured", "devices");
  await pick(page, BACKUP);
  const dialog = page.getByRole("dialog", { name: "Importar respaldo" });
  await dialog.getByRole("button", { name: "Cancelar" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("heading", { name: "Tira LED tele" })).toHaveCount(0);
  // Se puede volver a elegir el mismo archivo
  await pick(page, BACKUP);
  await expect(dialog).toBeVisible();
});

test("sin credenciales de Tuya los focos quedan pendientes y se ofrece cargarlas", async ({ page }) => {
  await open(page, "configured", "devices");
  await page.evaluate(() => ((window as any).__openlightMock.state.credentials = null));
  await pick(page, BACKUP);
  await page.getByRole("dialog").getByRole("button", { name: "Importar", exact: true }).click();
  await backup(page).getByRole("button", { name: "Bajar claves de la nube" }).click();
  const error = backup(page).getByRole("alert");
  await expect(error).toContainText("Faltan las credenciales de Tuya");
  await expect(backup(page).getByRole("button", { name: "Bajar claves de la nube" })).toBeVisible();
  await expectAccessible(page, "claves pendientes sin credenciales");
  await backup(page).getByRole("button", { name: "Cargar Access ID y Secret" }).click();
  await expect(page).toHaveURL(/#\/onboarding/);
  await expect(page.getByLabel("Access ID", { exact: false }).first()).toBeVisible();
});

test("un archivo que no sirve da un error claro y ofrece elegir otro", async ({ page }) => {
  await open(page, "configured", "devices");
  await pick(page, "{ esto no es json");
  const error = backup(page).getByRole("alert");
  await expect(error).toContainText("No pude leer ese respaldo");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  const chooser = page.waitForEvent("filechooser");
  await error.getByRole("button", { name: "Elegir otro archivo" }).click();
  await (await chooser).setFiles({ name: "nuevo.json", mimeType: "application/json", buffer: Buffer.from(JSON.stringify({ ...BACKUP, version: 2 })) });
  await expect(backup(page).getByRole("alert")).toContainText("Ese respaldo es de otra versión");
  await expect(backup(page).getByRole("alert")).toContainText("formato 2");
});

test("en una compu nueva: se sale del asistente y se importa en Dispositivos", async ({ page }) => {
  await open(page, "fresh");
  await page.getByRole("button", { name: "Seguir después" }).click();
  await page.getByRole("link", { name: "Dispositivos" }).click();
  await expect(backup(page).getByRole("button", { name: "Exportar respaldo" })).toBeDisabled();
  await pick(page, BACKUP);
  const dialog = page.getByRole("dialog", { name: "Importar respaldo" });
  await expect(dialog).toContainText("Focos nuevos: 2");
  await dialog.getByRole("button", { name: "Importar", exact: true }).click();
  await expect(backup(page)).toContainText("Respaldo importado. Focos: 2.");
  await expect(backup(page).getByRole("button", { name: "Exportar respaldo" })).toBeEnabled();
});

test("en inglés también", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("openlight.locale", "en"));
  await page.goto("/?scenario=configured&fast=1#/devices");
  const region = page.getByRole("region", { name: "Backup" });
  await expect(region.getByRole("button", { name: "Export backup" })).toBeVisible();
  await expect(region.getByRole("button", { name: "Import backup" })).toBeVisible();
  await expect(region).toContainText("doesn't include your bulbs' keys");
});
