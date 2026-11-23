import { describe, expect, it } from "vitest";
import es from "../content/guide/es.json";
import en from "../content/guide/en.json";
import type { HelpTopic, Region } from "./bindings";

function shape(o: unknown): unknown {
  if (Array.isArray(o)) return o.length && typeof o[0] === "object" ? o.map(shape) : `array:${o.length}`;
  if (typeof o === "object" && o !== null) return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, shape(v)]));
  return typeof o;
}

describe("contenido editable de la guía", () => {
  it("es y en tienen la misma estructura", () => {
    expect(shape(en)).toEqual(shape(es));
  });

  it("hay una guía para cada tema de ayuda del contrato", () => {
    const topics: HelpTopic[] = ["tuya_account", "cloud_project", "data_center", "link_smart_life", "iot_core_trial", "device_troubleshooting", "permissions"];
    for (const t of topics) expect(es.topics[t].steps.length).toBeGreaterThan(0);
  });

  it("cada Data Center del selector es una región válida", () => {
    const regions: Region[] = ["us", "us_east", "eu", "eu_west", "china", "india", "singapore"];
    for (const d of es.data_centers) expect(regions).toContain(d.region);
  });
});
