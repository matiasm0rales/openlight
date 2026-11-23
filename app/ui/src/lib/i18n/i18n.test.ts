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
