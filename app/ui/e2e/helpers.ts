import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

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
