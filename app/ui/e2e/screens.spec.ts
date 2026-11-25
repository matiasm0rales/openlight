import { expect, test } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

test("panel: tarjetas por foco, colores en vivo y el 914 con 'Renovar clave'", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Todo en pausa");
  await expectAccessible(page, "panel apagado");

  await page.getByRole("switch", { name: "Arrancar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Tus focos siguen la pantalla");
  const living = page.getByRole("article", { name: "Foco living" });
  const desk = page.getByRole("article", { name: "Lámpara escritorio" });
  await expect(living.getByText("Conectado", { exact: true })).toBeVisible();
  // El eclipse describe pantalla y foco para lectores de pantalla.
  await expect(living.getByRole("img", { name: /^Pantalla: .+\. Foco: .+\.$/ })).toBeVisible();
  await expect(living.getByText(/\d+ ms/)).toBeVisible();

  const error = desk.getByRole("alert");
  await expect(error).toContainText("El foco rechazó la clave");
  await expectAccessible(page, "panel con error 914");
  await error.getByRole("button", { name: "Renovar clave" }).click();
  await expect(desk.getByRole("alert")).toHaveCount(0);
  await expect(desk.getByText("Conectado", { exact: true })).toBeVisible();

  // Brillo rápido de un foco.
  const bright = living.getByRole("slider", { name: "Brillo" });
  await bright.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(bright).toHaveAttribute("aria-valuetext", "99 %");
});

test("calibración: mismas teclas que calibrar.py, modo rápido y guardar", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await page.getByRole("article", { name: "Foco living" }).getByRole("button", { name: "Calibrar" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Rojo");
  await expect(page.getByRole("heading", { level: 1 })).toContainText("1 / 12");
  const hue = page.getByRole("slider", { name: "Tono del foco" });
  const sat = page.getByRole("slider", { name: "Saturación del foco" });
  const startHue = await hue.getAttribute("value");
  await page.locator("body").press("ArrowRight");
  await expect(hue).not.toHaveAttribute("value", startHue ?? "");
  await page.locator("body").press("ArrowUp");
  await expect(sat).toHaveAttribute("aria-valuetext", /\d+ %/);
  await page.locator("body").press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Naranja");
  await expectAccessible(page, "calibración");

  await page.getByRole("button", { name: "Rápido (6)" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("1 / 6");
  await page.getByRole("button", { name: "Ir a Verde" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Verde");

  // Prueba A/B guiada.
  await page.getByRole("button", { name: "Prueba A/B" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Modo de color");
  await expect(page.getByText("Para comparar, el efecto tiene que estar andando.")).toBeVisible();
  await page.getByRole("button", { name: "Volver a la calibración" }).click();

  await page.getByRole("button", { name: "Guardar y salir" }).click();
  await expect(page).toHaveURL(/#\/dashboard/);
});

test("prueba A/B: lo que se prueba es temporal hasta que se elige", async ({ page }) => {
  const modeOf = () => page.evaluate(() => (window as any).__openlightMock.state.deviceOverrides["bf3a9c1e7d2b4f6a8c0e1d"]?.color_mode ?? null);
  await open(page, "configured", "dashboard");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await page.getByRole("article", { name: "Foco living" }).getByRole("button", { name: "Calibrar" }).click();
  await page.getByRole("button", { name: "Prueba A/B" }).click();
  await page.getByRole("button", { name: "Empezar" }).click();
  await expect(page.getByText("A", { exact: true }).locator("..")).toContainText("Dominante");
  await expect.poll(modeOf).toBe("dominant");

  // Salir sin elegir: el foco vuelve a heredar el valor global (no tenía uno propio).
  await page.getByRole("button", { name: "Volver a la calibración" }).click();
  await expect.poll(modeOf).toBeNull();

  // Elegir sí lo guarda.
  await page.getByRole("button", { name: "Prueba A/B" }).click();
  await page.getByRole("button", { name: "Empezar" }).click();
  await expect(page.getByRole("button", { name: "Me quedo con B" })).toBeVisible({ timeout: 15_000 });
  await page.getByRole("button", { name: "Me quedo con B" }).click();
  await expect.poll(modeOf).toBe("average");
  await page.getByRole("button", { name: "Volver a la calibración" }).click();
  await expect.poll(modeOf).toBe("average");
  // Con el efecto andando la calibración del foco no se abre: no hay nada que guardar.
  await page.getByRole("button", { name: "Salir sin guardar" }).click();
  await expect(page).toHaveURL(/#\/dashboard/);
});

test("control manual", async ({ page }) => {
  await open(page, "configured", "manual");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Control manual");
  await page.getByRole("button", { name: "Azul" }).click();
  await expect(page.getByRole("button", { name: "Azul" })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("radio", { name: "Blanco" }).check();
  await expect(page.getByRole("slider", { name: "Temperatura" })).toBeVisible();
  // La lámpara tiene la clave rechazada: el error trae su arreglo.
  await page.getByLabel("Foco", { exact: true }).selectOption({ label: "Lámpara escritorio" });
  await page.getByRole("slider", { name: "Brillo" }).focus();
  await page.keyboard.press("ArrowLeft");
  await expect(page.getByRole("alert")).toContainText("El foco rechazó la clave");
  await expectAccessible(page, "control manual");
});

test("dispositivos: renovar clave, IP fija y DPs", async ({ page }) => {
  await open(page, "configured", "devices");
  const desk = page.locator("li.device", { hasText: "Lámpara escritorio" });
  await expect(desk.getByRole("table")).toContainText("v2_hsv12");
  await desk.getByRole("button", { name: "Renovar clave" }).click();
  await expect(desk.getByText("Clave renovada. El foco ya está conectado.")).toBeVisible();

  await desk.getByRole("button", { name: "Editar" }).click();
  await desk.getByRole("radio", { name: "Fija" }).check();
  await desk.getByRole("textbox", { name: "Fija" }).fill("192.168.1.99");
  await desk.getByRole("button", { name: "Guardar" }).click();
  await expect(desk).toContainText("192.168.1.99");
  await expectAccessible(page, "dispositivos");

  await desk.getByRole("button", { name: "Quitar" }).click();
  const dialog = page.getByRole("dialog", { name: "¿Quitar Lámpara escritorio?" });
  await dialog.getByRole("button", { name: "Quitar" }).click();
  await expect(page.locator("li.device", { hasText: "Lámpara escritorio" })).toHaveCount(0);
});

test("diagnóstico: gráficos, salud del enlace, log y exportar", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await page.getByRole("link", { name: "Diagnóstico" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Diagnóstico");
  await expect(page.locator("figcaption", { hasText: "Capturas por segundo (objetivo 20)" })).toBeVisible({ timeout: 5000 });
  await expect(page.getByRole("region", { name: "Salud de cada enlace" })).toContainText("El foco cerró la conexión");
  const log = page.getByRole("log", { name: "Registro" });
  await expect(log).toContainText("la clave fue rechazada (914)");
  await page.getByRole("checkbox", { name: "Error" }).uncheck();
  await expect(log).not.toContainText("la clave fue rechazada (914)");
  await page.getByRole("searchbox", { name: "Buscar en el registro" }).fill("reconectado");
  await expect(log).toContainText("reconectado");
  await page.getByRole("button", { name: "Exportar diagnóstico" }).click();
  await expect(page.getByText(/Guardado en .*openlight-diagnostico\.zip/)).toBeVisible();
  await expectAccessible(page, "diagnóstico");
});

test("tray: menú y escenarios del mock", async ({ page }) => {
  await open(page, "configured", "tray");
  const menu = page.getByRole("group", { name: "Menú de OpenLight" });
  await menu.getByRole("button", { name: "Arrancar" }).click();
  await expect(menu.getByRole("button", { name: "Detener" })).toBeVisible();
  await menu.getByRole("button", { name: "Juegos" }).click();
  await expect(menu.getByRole("button", { name: /Juegos/ })).toHaveAttribute("aria-pressed", "true");
  await menu.getByRole("button", { name: "Salir (restaura los focos)" }).click();
  await expect(page.getByText("Los focos volvieron a su estado.")).toBeVisible();
  await expectAccessible(page, "tray");
});

test("ayuda y errores: show_help abre la guía (contenido editable)", async ({ page }) => {
  await open(page, "configured", "dashboard");
  await page.getByRole("switch", { name: "Arrancar" }).click();
  await page.getByRole("article", { name: "Lámpara escritorio" }).getByRole("button", { name: "Ver guía: El foco no responde" }).click();
  const dialog = page.getByRole("dialog", { name: "El foco no responde" });
  await expect(dialog).toContainText("Cerrá la app Smart Life");
  await expectAccessible(page, "guía de ayuda");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("tema oscuro, inglés y pantalla chica siguen siendo accesibles", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await open(page, "configured", "dashboard");
  await page.getByRole("button", { name: "EN" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Everything paused");
  await expectAccessible(page, "panel oscuro en inglés");
  await page.getByRole("button", { name: "Light theme" }).click();
  await expectAccessible(page, "panel claro forzado");
  await page.setViewportSize({ width: 390, height: 800 });
  await page.getByRole("link", { name: "Settings" }).click();
  await expectAccessible(page, "ajustes en pantalla chica");
});
