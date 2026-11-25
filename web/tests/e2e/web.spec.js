import { expect, test } from "@playwright/test";
import { RELEASES_URL, UA, open } from "./helpers.js";

const main = (page) => page.locator("#download-main");

for (const [os, name, file] of [
  ["macos", "macOS", "_universal.dmg"],
  ["windows", "Windows", "_x64-setup.exe"],
  ["linux", "Linux", "_amd64.AppImage"],
]) {
  test.describe(name, () => {
    test.use({ userAgent: UA[os] });

    test("ofrece su instalador y abre sus instrucciones", async ({ page }) => {
      await open(page);
      await expect(main(page)).toHaveText(`Descargar para ${name}`);
      await expect(main(page)).toHaveAttribute("href", new RegExp(`${file}$`));
      await expect(page.locator("#download-status")).toContainText("Versión 0.1.0");
      await expect(page.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
      await expect(page.getByRole("tabpanel")).toHaveAttribute("id", `panel-${os}`);
      // Los links chicos son los otros dos sistemas.
      await expect(page.locator("#download-others a:visible")).toHaveCount(2);
      await expect(page.locator(`#download-others a[data-os="${os}"]`)).toBeHidden();
    });
  });
}

for (const [caso, api] of [
  ["no hay releases", 404],
  ["se agotó el límite de consultas", 403],
  ["no hay conexión con la API", "caida"],
  ["la respuesta no trae instaladores", { tag_name: "v0.1.0", assets: [] }],
]) {
  test.describe(`cuando ${caso}`, () => {
    test.use({ userAgent: UA.macos });

    test("dice «Próximamente» y manda a la página de releases", async ({ page }) => {
      await open(page, { api });
      await expect(main(page)).toHaveText("Próximamente para macOS");
      await expect(main(page)).toHaveAttribute("href", RELEASES_URL);
      await expect(page.locator("a[data-download='dmg']")).toHaveText("Próximamente (.dmg)");
      await expect(page.locator("a[data-download='dmg']")).toHaveAttribute("href", RELEASES_URL);
    });
  });
}

test.describe("en un celular", () => {
  test.use({ userAgent: UA.android, viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test("explica que es para computadoras y ofrece los tres sistemas", async ({ page }) => {
    await open(page);
    await expect(main(page)).toHaveText("Ver todas las descargas");
    await expect(page.locator("#download-status")).toContainText("OpenLight es para computadoras");
    await expect(page.locator("#download-others a:visible")).toHaveCount(3);
  });
});

test.describe("idioma", () => {
  test.use({ userAgent: UA.macos });

  test("se cambia con el selector, se recuerda y vuelve al español", async ({ page }) => {
    await open(page);
    await page.getByRole("button", { name: "EN, English" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your bulbs, in the color of your screen");
    await expect(main(page)).toHaveText("Download for macOS");
    await expect(page).toHaveTitle(/your bulbs follow/);
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await page.getByRole("button", { name: "ES, español" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos, del color de tu pantalla");
    await expect(page.locator(".beta").first()).toContainText("Windows y Linux están en beta");
  });

  test("un enlace con ?lang=en abre en inglés y el selector lo puede cambiar", async ({ page }) => {
    await open(page, { path: "/?lang=en" });
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await page.getByRole("button", { name: "ES, español" }).click();
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "es");
  });

  test.describe("con el navegador en inglés", () => {
    test.use({ locale: "en-US" });

    test("arranca en inglés", async ({ page }) => {
      await open(page);
      await expect(main(page)).toHaveText("Download for macOS");
    });
  });
});

test("las pestañas de instalación se manejan con el teclado", async ({ page }) => {
  await open(page);
  await page.getByRole("tab", { name: "macOS" }).click();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Windows" })).toBeFocused();
  await expect(page.getByRole("tabpanel")).toHaveAttribute("id", "panel-windows");
  await page.keyboard.press("End");
  await expect(page.getByRole("tabpanel")).toHaveAttribute("id", "panel-linux");
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tabpanel")).toHaveAttribute("id", "panel-macos");
});

test("el tema sigue al sistema y la elección se recuerda", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await open(page);
  const background = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  await expect.poll(background).toBe("rgb(10, 9, 24)");
  await page.getByRole("button", { name: "Usar tema claro" }).click();
  await expect.poll(background).toBe("rgb(247, 245, 255)");
  await page.reload();
  await expect.poll(background).toBe("rgb(247, 245, 255)");
  await expect(page.getByRole("button", { name: "Usar tema oscuro" })).toBeVisible();
});

test("la demo cambia de escena sola, y elegir una la deja quieta", async ({ page }) => {
  await page.clock.install();
  await open(page);
  const html = page.locator("html");
  await expect(page.locator("#demo-play")).toHaveText("Pausar animaciones");
  await page.clock.runFor(5000);
  await expect(html).toHaveAttribute("data-scene", "oceano");
  await page.getByRole("button", { name: "Selva" }).click();
  await page.clock.runFor(20000);
  await expect(html).toHaveAttribute("data-scene", "selva");
  // Elegir una escena detiene el pase de escenas, no el resto de las animaciones.
  await expect(html).toHaveAttribute("data-motion", "on");
});

const moving = (page) => page.evaluate(() => document.getAnimations().filter((a) => a.playState === "running").length);

test("un botón pausa todas las animaciones y la elección se recuerda", async ({ page }) => {
  await open(page);
  await expect.poll(() => moving(page)).toBeGreaterThan(3);
  await page.getByRole("button", { name: "Pausar animaciones" }).click();
  await expect.poll(() => moving(page)).toBe(0);
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-motion", "off");
  await page.getByRole("button", { name: "Activar animaciones" }).click();
  await expect.poll(() => moving(page)).toBeGreaterThan(3);
});

test("con movimiento reducido todo arranca quieto", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.clock.install();
  await open(page);
  await expect(page.locator("#demo-play")).toHaveText("Activar animaciones");
  await page.clock.runFor(20000);
  await expect(page.locator("html")).toHaveAttribute("data-scene", "atardecer");
  expect(await moving(page)).toBe(0);
});

test.describe("sin JavaScript", () => {
  test.use({ javaScriptEnabled: false });

  test("se ven las tres guías y las descargas llevan a GitHub", async ({ page }) => {
    await page.goto("/");
    await expect(main(page)).toHaveAttribute("href", RELEASES_URL);
    for (const id of ["#panel-macos", "#panel-windows", "#panel-linux"]) await expect(page.locator(id)).toBeVisible();
    await expect(page.getByRole("tab")).toHaveCount(0);
    await expect(page.locator("#demo-play")).toBeHidden();
  });
});

test("carga sin errores ni bloqueos de la política de seguridad", async ({ page }) => {
  const problems = [];
  page.on("console", (message) => message.type() === "error" && problems.push(message.text()));
  page.on("pageerror", (error) => problems.push(error.message));
  page.on("response", (response) => response.status() >= 400 && problems.push(`${response.status()} ${response.url()}`));
  const external = [];
  page.on("request", (request) => !request.url().startsWith("http://localhost") && external.push(request.url()));
  await open(page);
  await expect(main(page)).toHaveAttribute("href", /download/);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForLoadState("networkidle");
  expect(problems).toEqual([]);
  // Lo único que sale de la web es la consulta de la última versión.
  expect(external).toEqual(["https://api.github.com/repos/matiasm0rales/openlight/releases/latest"]);
});
