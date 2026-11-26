import { execSync } from "node:child_process";
import { readFileSync, readdirSync, rmSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";

// La CSP de la app, probada de verdad: la UI compilada como para producción, servida con la
// política de `tauri.conf.json` y con cualquier bloqueo contado como falla. (`pnpm dev` y
// `tauri dev` no sirven para esto: la CSP solo se aplica a la UI empaquetada.)

const UI = fileURLToPath(new URL("..", import.meta.url));
const CSP: string = JSON.parse(readFileSync(join(UI, "../src-tauri/tauri.conf.json"), "utf8")).app.security.csp;
// Un origen inventado en loopback: Playwright contesta los pedidos, no hace falta un servidor.
const ORIGIN = "http://localhost:4599";
const TYPES: Record<string, string> = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};

let dist: string;

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  dist = mkdtempSync(join(tmpdir(), "openlight-csp-"));
  execSync(`pnpm exec vite build --outDir "${dist}" --emptyOutDir --logLevel warn`, { cwd: UI, stdio: "inherit" });
});

test.afterAll(() => {
  if (dist) rmSync(dist, { recursive: true, force: true });
});

/** Sirve la compilación con la CSP y junta cada cosa que la política bloquee. */
async function serve(page: Page): Promise<string[]> {
  const blocked: string[] = [];
  await page.exposeFunction("__cspViolation", (what: string) => blocked.push(what));
  await page.addInitScript(() => {
    document.addEventListener("securitypolicyviolation", (e) => {
      (window as any).__cspViolation(`${e.violatedDirective}: ${e.blockedURI} ${e.sourceFile ?? ""}:${e.lineNumber}`);
    });
  });
  await page.route(`${ORIGIN}/**`, async (route) => {
    const path = new URL(route.request().url()).pathname;
    const file = join(dist, path === "/" ? "index.html" : path);
    try {
      await route.fulfill({
        body: readFileSync(file),
        contentType: TYPES[extname(file)] ?? "application/octet-stream",
        headers: { "content-security-policy": CSP },
      });
    } catch {
      await route.fulfill({ status: 404, body: "" });
    }
  });
  return blocked;
}

/** Todas las imágenes visibles cargaron (una bloqueada queda con tamaño 0). */
async function expectImages(page: Page, label: string) {
  await expect
    .poll(() => page.evaluate(() => [...document.images].filter((i) => !i.complete || i.naturalWidth === 0).map((i) => i.src.slice(0, 80))), { message: label })
    .toEqual([]);
}

const go = async (page: Page, hash: string) => {
  await page.evaluate((h) => (location.hash = h), `#/${hash}`);
  await expect(page.locator("main")).not.toContainText("Cargando…");
  await expect(page.locator("main h1, main h2").first()).toBeVisible();
};

test("la política es estricta: sin comodines, sin inline y sin eval", () => {
  const policy = new Map(CSP.split(";").map((d) => d.trim().split(/\s+/)).map(([name, ...values]) => [name, values]));
  expect(Object.fromEntries(policy)).toEqual({
    "default-src": ["'none'"],
    "script-src": ["'self'"],
    "style-src": ["'self'"],
    // `data:`: las vistas previas de los monitores y las ilustraciones chicas que Vite incrusta.
    "img-src": ["'self'", "data:"],
    "font-src": ["'self'"],
    "connect-src": ["ipc:", "http://ipc.localhost"],
    "object-src": ["'none'"],
    "base-uri": ["'none'"],
    "form-action": ["'none'"],
    "frame-ancestors": ["'none'"],
  });
});

test("la compilación no incrusta fuentes (la política solo admite las locales)", () => {
  const css = readdirSync(join(dist, "assets")).filter((f) => f.endsWith(".css"));
  expect(css.length).toBeGreaterThan(0);
  for (const file of css) expect(readFileSync(join(dist, "assets", file), "utf8")).not.toContain("data:font");
});

test("el detector ve un bloqueo: un script y un estilo metidos a mano no corren", async ({ page }) => {
  const blocked = await serve(page);
  await page.goto(`${ORIGIN}/?scenario=configured&fast=1`);
  await expect(page.locator("main h1")).toBeVisible();
  expect(blocked).toEqual([]);
  const ran = await page.evaluate(() => {
    const script = document.createElement("script");
    script.textContent = "window.__injected = true";
    document.body.append(script);
    const marker = document.createElement("div");
    marker.innerHTML = '<span id="styled" style="position: fixed">x</span>';
    document.body.append(marker);
    return { script: Boolean((window as any).__injected), style: getComputedStyle(document.getElementById("styled")!).position };
  });
  expect(ran).toEqual({ script: false, style: "static" });
  await expect.poll(() => blocked.length).toBeGreaterThanOrEqual(2);
});

test("todas las pantallas andan sin que la política bloquee nada", async ({ page }) => {
  const blocked = await serve(page);
  await page.goto(`${ORIGIN}/?scenario=configured&fast=1&update=available#/dashboard`);
  await expect(page.getByRole("region", { name: /Hay una versión nueva/ })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  expect(await page.evaluate(() => document.fonts.check('16px "Plus Jakarta Sans Variable"'))).toBe(true);
  await expectImages(page, "panel");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  // Los colores en vivo van por `style:` de Svelte: tienen que llegar al elemento.
  await expect.poll(() => page.evaluate(() => document.querySelector<HTMLElement>(".shell")!.style.getPropertyValue("--ambient"))).not.toBe("");

  for (const screen of ["settings", "devices", "diagnostics", "manual", "tray"]) {
    await go(page, screen);
    await expectImages(page, screen);
  }
  await go(page, "calibrate");
  // Con el efecto andando el foco está ocupado: la calibración ofrece sacarlo.
  await page.getByRole("button", { name: "Sacar del efecto" }).click();
  await expect(page.getByRole("slider", { name: "Tono del foco" })).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.querySelector<HTMLElement>(".calibration")!.style.getPropertyValue("--screen"))).not.toBe("");
  await page.getByRole("button", { name: "Prueba A/B" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Modo de color");
  expect(blocked).toEqual([]);
});

test("el asistente anda sin bloqueos: guías, ilustraciones y vista previa de los monitores", async ({ page }) => {
  const blocked = await serve(page);
  await page.goto(`${ORIGIN}/?scenario=fresh&fast=1`);
  const next = page.getByTestId("next");
  const title = page.locator("#step-title");
  await expect(title).toHaveText("Tus focos, siguiendo tu pantalla");
  await next.click();
  await page.getByRole("button", { name: "Abrir Grabación de pantalla" }).click();
  await expect(page.getByText("Concedido")).toHaveCount(2, { timeout: 5000 });
  await next.click();
  for (const confirm of ["Ya tengo cuenta en iot.tuya.com", "Ya creé el proyecto", "Ya vinculé mi cuenta de Smart Life"]) {
    await expect(page.locator("main img").first()).toBeVisible();
    await expectImages(page, confirm);
    if (confirm === "Ya creé el proyecto") await page.getByLabel("Argentina, Uruguay, Chile, México y el resto de América").check();
    await page.getByRole("button", { name: "¿Cómo?" }).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.getByRole("button", { name: "Cerrar" }).click();
    await page.getByLabel(confirm).check();
    await next.click();
  }
  await page.getByRole("textbox", { name: "Access ID" }).fill("p9xk2wq7vd4mfa1ztc8e");
  await page.getByLabel("Access Secret", { exact: true }).fill("0123456789abcdef0123456789abcdef");
  await page.getByRole("button", { name: "Probar", exact: true }).click();
  await expect(page.getByText(/¡Anda! Conectado con Tuya/)).toBeVisible();
  await next.click();
  await expect(page.getByText("encontrados en tu red")).toBeVisible();
  await next.click();
  for (const name of ["Foco living", "Lámpara escritorio"]) {
    const card = page.locator("li", { hasText: name });
    await card.getByRole("button", { name: "Probar rojo, verde y azul" }).click();
    await card.getByRole("button", { name: "Sí, los vi" }).click();
  }
  await next.click();
  await expect(title).toHaveText("Monitor y zonas");
  const preview = page.locator('main img[src^="data:image/png"]').first();
  await expect(preview).toBeVisible();
  await expectImages(page, "vista previa");
  await expect(page.getByRole("slider", { name: "Zona de Foco living" })).toHaveAttribute("style", /left/);
  await next.click();
  await next.click();
  await expect(title).toHaveText("¡Listo!");
  expect(blocked).toEqual([]);
});
