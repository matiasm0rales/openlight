// La app real: con Tauri presente no se instala el backend falso. Acá "Tauri" es lo mínimo para
// que la UI arranque (sin focos).
import { expect, test } from "@playwright/test";
import { CATALOG, defaultSettings } from "../src/lib/mocks/catalog";

test.beforeEach(async ({ page }) => {
  const answers = {
    onboarding_state: { step: "done", completed: [], has_credentials: true, devices: [] },
    devices_list: [],
    settings_get: { catalog: CATALOG, effective: defaultSettings(), global: defaultSettings(), devices: [], active_profile: null },
    profiles_list: [],
    session_state: "idle",
  };
  await page.addInitScript((answers: Record<string, unknown>) => {
    let id = 0;
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => (cmd === "plugin:event|listen" ? ++id : (answers[cmd] ?? null)),
      transformCallback: () => ++id,
      unregisterCallback: () => {},
    };
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, answers);
});

test("en la app real no está la maqueta del tray", async ({ page }) => {
  await page.goto("/#/dashboard");
  const nav = page.getByRole("navigation", { name: "Secciones" });
  await expect(nav.getByRole("link", { name: "Diagnóstico" })).toBeVisible();
  await expect(nav.getByRole("link", { name: "Menú del tray" })).toHaveCount(0);

  // Ni escribiendo la ruta: cae en el panel
  await page.goto("/#/tray");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Todo en pausa");
  await expect(page.getByRole("group", { name: "Menú de OpenLight" })).toHaveCount(0);
});
