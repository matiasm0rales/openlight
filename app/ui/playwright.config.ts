import { defineConfig, devices } from "@playwright/test";

// La UI corre en el navegador contra el backend falso (src/lib/mocks): no hace falta Tauri.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: "http://localhost:1420",
    locale: "es-AR",
    // Sin animaciones: axe mide el contraste final, no un elemento a medio aparecer.
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    ...devices["Desktop Chrome"],
    viewport: { width: 1280, height: 860 },
  },
  webServer: {
    command: "pnpm dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
