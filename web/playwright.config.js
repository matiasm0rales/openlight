import { defineConfig, devices } from "@playwright/test";

// Puerto distinto al de `npm run dev` para que las pruebas no se crucen con un servidor abierto.
const PORT = 4181;

export default defineConfig({
  testDir: "tests/e2e",
  forbidOnly: Boolean(process.env.CI),
  reporter: "list",
  use: { baseURL: `http://localhost:${PORT}`, locale: "es-AR" },
  webServer: {
    command: "node scripts/serve.mjs",
    url: `http://localhost:${PORT}`,
    env: { PORT: String(PORT) },
    reuseExistingServer: false,
  },
  projects: [{ name: "chromium", use: devices["Desktop Chrome"] }],
});
