import AxeBuilder from "@axe-core/playwright";
import { expect, type Locator, type Page } from "@playwright/test";

export async function open(page: Page, scenario: "fresh" | "configured", hash = "", query = "") {
  await page.goto(`/?scenario=${scenario}&fast=1${query ? `&${query}` : ""}${hash ? `#/${hash}` : ""}`);
  await expect(page.locator("main")).not.toContainText("Cargando…");
}

/** Auditoría de accesibilidad: no se aceptan violaciones serias ni críticas. */
export async function expectAccessible(page: Page, label: string) {
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"]).analyze();
  const bad = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  const summary = bad.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).slice(0, 4).join(" | ")}`);
  expect(summary, `axe en ${label}`).toEqual([]);
}

/** Más estricta que `expectAccessible`: ninguna violación, del impacto que sea. */
export async function expectNoViolations(page: Page, label: string) {
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"]).analyze();
  const summary = results.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).slice(0, 4).join(" | ")}`);
  expect(summary, `axe en ${label}`).toEqual([]);
}

/**
 * Contraste WCAG entre un color del elemento y el fondo que se ve detrás, para lo que axe no mide
 * (íconos, el contorno de foco y los controles con degradados). `what`: «text» (el texto o el ícono
 * contra su propio fondo), «outline» (el contorno contra el fondo de alrededor) o dos propiedades
 * CSS propias, la de adelante y la de atrás.
 */
export function contrast(target: Locator, what: "text" | "outline" | [string, string]): Promise<number> {
  return target.evaluate((el, what) => {
    const canvas = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
    // El canvas resuelve cualquier sintaxis de color (`color-mix`, `rgb(… / a)`) a canales.
    const paint = (colors: string[]) => {
      canvas.clearRect(0, 0, 1, 1);
      for (const c of colors) {
        canvas.fillStyle = c;
        canvas.fillRect(0, 0, 1, 1);
      }
      return [...canvas.getImageData(0, 0, 1, 1).data].slice(0, 3);
    };
    const backgrounds = (from: Element | null) => {
      const stack: string[] = [];
      for (let n = from; n; n = n.parentElement) stack.unshift(getComputedStyle(n).backgroundColor);
      return ["#fff", ...stack];
    };
    const luminance = (rgb: number[]) => {
      const [r, g, b] = rgb.map((v) => (v / 255 <= 0.04045 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4));
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const style = getComputedStyle(el);
    let front: string[];
    let back: string[];
    if (what === "text") {
      back = backgrounds(el);
      front = [...back, style.color];
    } else if (what === "outline") {
      back = backgrounds(el.parentElement);
      front = [...back, style.outlineColor];
    } else {
      back = [...backgrounds(el), style.getPropertyValue(what[1])];
      front = [...back, style.getPropertyValue(what[0])];
    }
    const [a, b] = [luminance(paint(front)), luminance(paint(back))];
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  }, what);
}

/** El elemento con el foco de teclado tiene un contorno que se distingue (WCAG 1.4.11 y 2.4.7). */
export async function expectVisibleFocus(target: Locator) {
  await expect(target).toBeFocused();
  await expect(target).toHaveCSS("outline-style", "solid");
  expect(parseFloat(await target.evaluate((el) => getComputedStyle(el).outlineWidth))).toBeGreaterThanOrEqual(2);
  expect(await contrast(target, "outline")).toBeGreaterThanOrEqual(3);
}
