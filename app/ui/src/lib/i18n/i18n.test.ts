import { describe, expect, it } from "vitest";
import es from "./es.json";
import en from "./en.json";

function keys(obj: unknown, prefix = ""): string[] {
  if (typeof obj !== "object" || obj === null) return [prefix];
  return Object.entries(obj).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k));
}

describe("i18n", () => {
  it("es y en tienen exactamente las mismas claves", () => {
    expect(keys(en).sort()).toEqual(keys(es).sort());
  });

  it("ningún texto está vacío", () => {
    for (const locale of [es, en]) {
      const vacios = keys(locale).filter(
        (k) => k.split(".").reduce<any>((o, p) => o?.[p], locale) === "",
      );
      expect(vacios).toEqual([]);
    }
  });
});
