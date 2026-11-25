import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { open } from "./helpers.js";

const SIZES = {
  escritorio: { width: 1280, height: 900 },
  celular: { width: 360, height: 780 },
};

async function violations(page) {
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  return result.violations.map((v) => `${v.id}: ${v.help} → ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

for (const colorScheme of ["light", "dark"]) {
  for (const [size, viewport] of Object.entries(SIZES)) {
    test.describe(`${colorScheme} en ${size}`, () => {
      test.use({ viewport, colorScheme, reducedMotion: "reduce" });

      test("pasa axe en cada guía y en los dos idiomas", async ({ page }) => {
        await open(page);
        for (const language of ["ES, español", "EN, English"]) {
          await page.getByRole("button", { name: language }).click();
          await page.locator("details").evaluateAll((all) => all.forEach((d) => (d.open = true)));
          for (const tab of ["macOS", "Windows", "Linux"]) {
            await page.getByRole("tab", { name: tab }).click();
            expect(await violations(page), `${language}, ${tab}`).toEqual([]);
          }
        }
      });

      test("no se desborda hacia los costados", async ({ page }) => {
        await open(page);
        for (const language of ["ES, español", "EN, English"]) {
          await page.getByRole("button", { name: language }).click();
          for (const tab of ["macOS", "Windows", "Linux"]) {
            await page.getByRole("tab", { name: tab }).click();
            const offenders = await page.evaluate(() => {
              const width = document.documentElement.clientWidth;
              // Lo que desplaza su propio contenido (los comandos) no cuenta, y la demo ilumina afuera.
              return [...document.querySelectorAll("body *:not(pre, pre *, .aurora, .aurora *, .skip, .stage *, .need-art *)")]
                .filter((el) => el.checkVisibility() && el.getBoundingClientRect().right > width + 0.5)
                .map((el) => `${el.tagName.toLowerCase()}.${el.getAttribute("class") ?? ""}`);
            });
            expect(offenders, `${language}, ${tab}`).toEqual([]);
          }
        }
      });
    });
  }
}
