import { afterEach, describe, expect, it, vi } from "vitest";
import { detect, localeFromTag } from "./index.svelte";
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

// Todas las claves que usa la UI existen en los dos idiomas: las literales (`t("...")`, sacadas del
// código) y las que se arman con un valor del contrato (`device_state.<kind>`, `fix.open_settings.<pane>`...).
const sources = import.meta.glob(["../../**/*.svelte", "../../**/*.ts", "!../../**/*.test.ts"], { eager: true, query: "?raw", import: "default" }) as Record<string, string>;

function lookup(root: unknown, key: string): unknown {
  return key.split(".").reduce<any>((o, p) => o?.[p], root);
}

describe("i18n: cobertura de la UI", () => {
  const literal = new Set<string>();
  for (const code of Object.values(sources)) for (const m of code.matchAll(/\bt\("([a-z_.0-9]+)"/g)) literal.add(m[1]);

  const families: Record<string, string[]> = {
    "onboarding.{}.short": ["welcome", "permissions", "tuya_account", "cloud_project", "link_app", "credentials", "choose_devices", "test_devices", "zones", "calibrate", "done"],
    "onboarding.{}.title": ["welcome", "permissions", "tuya_account", "cloud_project", "link_app", "credentials", "choose_devices", "test_devices", "zones", "calibrate", "done"],
    "onboarding.{}.lead": ["welcome", "permissions", "tuya_account", "cloud_project", "link_app", "credentials", "choose_devices", "test_devices", "zones", "calibrate", "done"],
    "onboarding.{}.needs": ["welcome", "permissions", "tuya_account", "cloud_project", "link_app", "credentials", "choose_devices", "test_devices", "zones", "calibrate", "done"],
    "onboarding.{}.image_alt": ["tuya_account", "cloud_project", "link_app"],
    "session.state.{}": ["idle", "connecting", "active", "paused", "restoring"],
    "session.explain.{}": ["idle", "connecting", "active", "paused", "restoring"],
    "device_state.{}": ["idle", "connecting", "active", "reconnecting", "paused", "restoring", "error"],
    "disconnect.{}": ["peer_closed", "timeout", "no_response", "key_rejected", "refused", "io"],
    "disconnect_help.{}": ["peer_closed", "timeout", "no_response", "key_rejected", "refused", "io"],
    "fix.open_settings.{}": ["screen_recording", "local_network", "firewall", "screen_cast_portal"],
    "permission.{}.title": ["screen_recording", "local_network", "firewall", "screen_cast_portal"],
    "permission.state.{}": ["granted", "denied", "not_determined", "not_applicable"],
    "help.{}.title": ["tuya_account", "cloud_project", "data_center", "link_smart_life", "iot_core_trial", "device_troubleshooting", "permissions"],
    "region.{}": ["us", "us_east", "eu", "eu_west", "china", "india", "singapore"],
    "zones.preset.{}": ["full", "left", "right", "top", "bottom", "center", "custom"],
    "settings.level.{}": ["basic", "advanced", "expert"],
    "settings.scope_note.{}": ["global", "device", "profile"],
    "log_level.{}": ["error", "warn", "info", "debug"],
    "nav.{}": ["dashboard", "settings", "devices", "diagnostics", "manual", "calibrate", "tray", "onboarding"],
    "capability.{}": ["colour", "white", "temperature", "realtime", "scenes"],
    "hue.{}": ["red", "orange", "yellow", "lime", "green", "teal", "cyan", "sky", "blue", "violet", "magenta", "pink"],
    "profile.{}": ["movies", "games", "work"],
  };
  const dynamic = Object.entries(families).flatMap(([pattern, values]) => values.map((v) => pattern.replace("{}", v)));

  for (const [name, locale] of [["es", es], ["en", en]] as const) {
    it(`${name}.json tiene todas las claves que usa la UI`, () => {
      const faltan = [...literal, ...dynamic].filter((k) => typeof lookup(locale, k) !== "string");
      expect(faltan).toEqual([]);
    });
  }
});

describe("idioma por defecto", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("el español en cualquier variante sale en español", () => {
    for (const tag of ["es", "es-AR", "es-419", "es-ES", "es_MX", "ES-ar"]) expect(localeFromTag(tag)).toBe("es");
  });

  it("el inglés en cualquier variante sale en inglés", () => {
    for (const tag of ["en", "en-US", "en-GB", "en_AU", "EN-us"]) expect(localeFromTag(tag)).toBe("en");
  });

  it("un idioma que la app no traduce sale en español, como el resto del producto", () => {
    for (const tag of ["pt-BR", "fr-FR", "de", "zh-Hans-CN", "ja", "", undefined, null]) expect(localeFromTag(tag)).toBe("es");
  });

  it("no confunde un código que solo empieza con «en» o «es»", () => {
    expect(localeFromTag("enm")).toBe("es");
    expect(localeFromTag("est-EE")).toBe("es");
  });

  it("detect() usa el idioma del sistema cuando no hay nada guardado", () => {
    vi.stubGlobal("navigator", { language: "es-419" });
    expect(detect()).toBe("es");
    vi.stubGlobal("navigator", { language: "en-US" });
    expect(detect()).toBe("en");
    vi.stubGlobal("navigator", { language: "pt-BR" });
    expect(detect()).toBe("es");
  });

  it("detect() respeta la elección guardada por encima del sistema", () => {
    vi.stubGlobal("navigator", { language: "en-US" });
    vi.stubGlobal("localStorage", { getItem: () => "es" });
    expect(detect()).toBe("es");
  });
});
