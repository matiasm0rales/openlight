import { readFileSync } from "node:fs";

export const RELEASE = JSON.parse(readFileSync(new URL("../fixtures/release-latest.json", import.meta.url), "utf8"));
export const RELEASES_URL = "https://github.com/matiasm0rales/openlight/releases";

export const UA = {
  macos: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
  windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
  android: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
};

/**
 * Abre la web sin tocar la red real: la API de GitHub se responde acá.
 * `api` puede ser un objeto (200), un número (código de error) o "caida" (sin conexión).
 */
export async function open(page, { api = RELEASE, path = "/" } = {}) {
  // Las client hints delatarían el procesador de la máquina que corre las pruebas.
  await page.addInitScript(() => Object.defineProperty(navigator, "userAgentData", { value: undefined }));
  await page.route("https://api.github.com/**", (route) => {
    if (api === "caida") return route.abort();
    const status = typeof api === "number" ? api : 200;
    const body = typeof api === "number" ? { message: "Not Found" } : api;
    return route.fulfill({ status, json: body, headers: { "access-control-allow-origin": "*" } });
  });
  await page.goto(path);
}
