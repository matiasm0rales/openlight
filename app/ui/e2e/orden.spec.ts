import { expect, test, type Page } from "@playwright/test";
import { expectAccessible, open } from "./helpers";

/** Lo que quedó guardado en el backend falso, en orden. */
const saved = (page: Page) =>
  page.evaluate(() =>
    (window as any).__openlightMock.state.devices.map((d: any) => ({ name: d.name, zones: d.zones.map((z: any) => `${z.display}:${z.preset}`) })),
  );

test("distribuciones: con cuatro focos se reparten las cuatro esquinas, en el orden del panel", async ({ page }) => {
  await open(page, "four", "devices");
  const layouts = page.getByRole("group", { name: "Distribución para todos los focos" });
  // Solo las que coinciden con la cantidad de focos.
  await expect(layouts.getByRole("button")).toHaveText([/Cuatro esquinas$/]);
  await expect(layouts).toContainText("1 Foco living, 2 Lámpara escritorio, 3 Foco repisa, 4 Foco techo");

  const corners = layouts.getByRole("button", { name: "Cuatro esquinas" });
  await expect(corners).toHaveAttribute("aria-pressed", "false");
  await corners.click();
  await expect(corners).toHaveAttribute("aria-pressed", "true");
  const display = "69733382";
  await expect
    .poll(async () => (await saved(page)).map((d: { zones: string[] }) => d.zones))
    .toEqual([[`${display}:top_left`], [`${display}:top_right`], [`${display}:bottom_left`], [`${display}:bottom_right`]]);

  // Después se ajusta cada uno a mano con el editor de siempre.
  const presets = page.getByRole("group", { name: "Atajos de zona para Foco living" });
  await expect(presets.getByRole("button", { name: "Arriba izquierda" })).toHaveAttribute("aria-pressed", "true");
  await presets.getByRole("button", { name: "Centro" }).click();
  await expect(corners).toHaveAttribute("aria-pressed", "false");
  await expect.poll(async () => (await saved(page))[0].zones).toEqual([`${display}:center`]);
  await expectAccessible(page, "distribuciones con cuatro focos");
});

test("distribuciones: con dos focos se ofrecen lados o arriba y abajo, sin tocar otros monitores", async ({ page }) => {
  await open(page, "configured", "devices");
  const layouts = page.getByRole("group", { name: "Distribución para todos los focos" });
  await expect(layouts.getByRole("button")).toHaveText([/Izquierda y derecha$/, /Arriba y abajo$/]);
  await expect(layouts.getByRole("button", { name: "Izquierda y derecha" })).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("radio", { name: /Pantalla integrada/ }).check({ force: true });
  await expect(layouts.getByRole("button", { name: "Izquierda y derecha" })).toHaveAttribute("aria-pressed", "false");
  await layouts.getByRole("button", { name: "Arriba y abajo" }).click();
  await expect
    .poll(async () => (await saved(page)).map((d: { zones: string[] }) => d.zones))
    .toEqual([
      ["69733382:left", "1:top"],
      ["69733382:right", "1:bottom"],
    ]);
  await expectAccessible(page, "distribuciones con dos focos");
});

// ---------- Orden de los focos en el panel ----------

const START = ["Foco living", "Lámpara escritorio", "Foco repisa", "Foco techo"];
const cards = (page: Page) => page.getByRole("list", { name: "Tus focos" }).getByRole("heading", { level: 3 });
const grip = (page: Page, name: string) => page.getByRole("button", { name: `Reordenar ${name}` });
const names = async (page: Page) => (await saved(page)).map((d: { name: string }) => d.name);

test("panel: reordenar con el teclado, con aviso y con el foco puesto donde estaba", async ({ page }) => {
  await open(page, "four", "dashboard");
  await expect(cards(page)).toHaveText(START);
  const living = grip(page, "Foco living");
  const status = page.locator("#reorder-status");
  await expect(status).toHaveAttribute("aria-live", "polite");

  await living.focus();
  await page.keyboard.press("ArrowDown");
  await expect(cards(page)).toHaveText(["Lámpara escritorio", "Foco living", "Foco repisa", "Foco techo"]);
  await expect(status).toHaveText("Foco living pasó al lugar 2 de 4.");
  await expect(living).toBeFocused();
  // Seguir apretando sigue moviendo el mismo foco.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(status).toHaveText("Foco living pasó al lugar 4 de 4.");
  await page.keyboard.press("ArrowDown");
  await expect(status).toHaveText("Foco living ya está al final.");
  await page.keyboard.press("ArrowUp");
  const moved = ["Lámpara escritorio", "Foco repisa", "Foco living", "Foco techo"];
  await expect(cards(page)).toHaveText(moved);
  await expect(living).toBeFocused();
  await expect.poll(() => names(page)).toEqual(moved);

  await grip(page, "Lámpara escritorio").press("ArrowUp");
  await expect(status).toHaveText("Lámpara escritorio ya está primero.");
  await expectAccessible(page, "panel reordenado con teclado");

  // Dispositivos y el tray usan la misma lista.
  await page.getByRole("link", { name: "Dispositivos" }).click();
  await expect(page.locator("li.device h2")).toHaveText(moved);
  await page.getByRole("link", { name: "Menú del tray" }).click();
  const menu = page.getByRole("group", { name: "Menú de OpenLight" });
  await expect(menu).toContainText(new RegExp(moved.join(".*")));
});

test("panel: «Mover antes» y «Mover después» sin arrastrar", async ({ page }) => {
  await open(page, "four", "dashboard");
  const living = grip(page, "Foco living");
  await expect(living).toHaveAttribute("aria-expanded", "false");
  await living.press("Enter");
  await expect(living).toHaveAttribute("aria-expanded", "true");
  const before = page.getByRole("button", { name: "Mover antes" });
  const after = page.getByRole("button", { name: "Mover después" });
  await expect(before).toHaveAttribute("aria-disabled", "true");
  await expectAccessible(page, "panel con los botones de mover");

  await after.click();
  await expect(cards(page)).toHaveText(["Lámpara escritorio", "Foco living", "Foco repisa", "Foco techo"]);
  await expect(after).toBeFocused();
  await expect(before).toHaveAttribute("aria-disabled", "false");
  await after.press("Enter");
  await expect(cards(page)).toHaveText(["Lámpara escritorio", "Foco repisa", "Foco living", "Foco techo"]);
  await before.press("Enter");
  await expect(page.locator("#reorder-status")).toHaveText("Foco living pasó al lugar 2 de 4.");

  await page.keyboard.press("Escape");
  await expect(after).toBeHidden();
  await expect(living).toBeFocused();
  // Tocar afuera también lo cierra.
  await living.click();
  await expect(after).toBeVisible();
  await page.getByRole("heading", { level: 1 }).click();
  await expect(after).toBeHidden();
});

/** El centro de un elemento, para llevar el puntero. */
async function centre(page: Page, target: ReturnType<typeof grip>) {
  const box = (await target.boundingBox())!;
  return [box.x + box.width / 2, box.y + box.height / 2] as const;
}

test("panel: arrastrar un foco muestra dónde cae y lo deja ahí", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 1800 });
  await open(page, "four", "dashboard");
  const techo = grip(page, "Foco techo");
  const target = page.getByRole("article", { name: "Foco living" });
  const mark = page.getByText("Foco techo va acá");

  await page.mouse.move(...(await centre(page, techo)));
  await page.mouse.down();
  await expect(mark).toHaveCount(0);
  await page.mouse.move(...(await centre(page, target)), { steps: 8 });
  await expect(mark).toBeVisible();
  // También a mitad de un arrastre se tiene que poder leer todo.
  await expectAccessible(page, "panel arrastrando");
  await page.mouse.up();
  const moved = ["Foco techo", "Foco living", "Lámpara escritorio", "Foco repisa"];
  await expect(cards(page)).toHaveText(moved);
  await expect(mark).toHaveCount(0);
  await expect.poll(() => names(page)).toEqual(moved);
  await expect(page.locator("#reorder-status")).toHaveText("Foco techo pasó al lugar 1 de 4.");
  // Soltar no abre los botones: fue un arrastre, no un toque.
  await expect(techo).toHaveAttribute("aria-expanded", "false");

  // Con Esc se cancela; soltar lejos de la lista tampoco cambia nada.
  await page.mouse.move(...(await centre(page, techo)));
  await page.mouse.down();
  await page.mouse.move(...(await centre(page, page.getByRole("article", { name: "Foco repisa" }))), { steps: 8 });
  await page.keyboard.press("Escape");
  await expect(mark).toHaveCount(0);
  await page.mouse.up();
  await expect(techo).toHaveAttribute("aria-expanded", "false");
  await page.mouse.move(...(await centre(page, techo)));
  await page.mouse.down();
  await page.mouse.move(20, 600, { steps: 8 });
  await expect(mark).toHaveCount(0);
  await page.mouse.up();
  await expect(cards(page)).toHaveText(moved);
  // Y el asa sigue andando con el teclado después de arrastrar.
  await techo.press("Enter");
  await expect(techo).toHaveAttribute("aria-expanded", "true");
});

test("panel: arrastrar hasta el borde desplaza la página", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 640 });
  await open(page, "four", "dashboard");
  const techo = grip(page, "Foco techo");
  await techo.scrollIntoViewIfNeeded();
  const scrolled = () => page.evaluate(() => window.scrollY);
  expect(await scrolled()).toBeGreaterThan(200);
  await page.mouse.move(...(await centre(page, techo)));
  await page.mouse.down();
  await page.mouse.move(500, 10, { steps: 8 });
  await expect.poll(scrolled, { timeout: 15_000 }).toBeLessThan(5);
  await page.mouse.move(...(await centre(page, page.getByRole("article", { name: "Foco living" }))), { steps: 4 });
  await page.mouse.up();
  await expect(cards(page).first()).toHaveText("Foco techo");
});

test("panel: si el orden no se puede guardar, vuelve a como estaba y lo avisa", async ({ page }) => {
  await open(page, "four", "dashboard");
  await page.evaluate(() => {
    const mock = (window as any).__openlightMock;
    const handle = mock.handle.bind(mock);
    mock.handle = async (cmd: string, args: unknown) => {
      if (cmd !== "devices_reorder") return handle(cmd, args);
      await new Promise((r) => setTimeout(r, 300));
      throw { code: "storage", title: "error.storage.title", detail: "error.storage.detail", params: {}, actions: [{ kind: "retry" }] };
    };
  });
  const living = grip(page, "Foco living");
  await living.press("ArrowDown");
  await expect(cards(page).first()).toHaveText("Lámpara escritorio");
  await expect(cards(page)).toHaveText(START);
  await expect(page.locator("#reorder-status")).toHaveText("No se pudo guardar el orden. Volvió a como estaba.");
  await expect(page.getByRole("alert")).toContainText("No pude guardar la configuración");
  await expect(living).toBeFocused();
});
