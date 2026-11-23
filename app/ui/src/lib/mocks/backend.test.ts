// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import { commands } from "../bindings";
import { installMocks } from "./backend";

afterEach(() => clearMocks());

const creds = (over: Partial<{ access_id: string; access_secret: string; region: "us" | "eu" }> = {}) => ({
  access_id: "p9xk2wq7vd4mfa1ztc8e",
  access_secret: "0123456789abcdef0123456789abcdef",
  region: "us" as const,
  ...over,
});

describe("backend falso (mismo contrato que ipc.rs)", () => {
  it("cada error trae código, claves i18n y al menos una acción", async () => {
    installMocks({ scenario: "fresh", fast: true });
    const cases = [
      creds({ access_secret: "corto" }),
      creds({ access_id: "corto" }),
      creds({ access_id: "expired-123456789" }),
      creds({ access_id: "nolink-123456789" }),
      creds({ region: "eu" }),
    ];
    const codes = [];
    for (const c of cases) {
      const e = await commands.cloudValidate(c).catch((x) => x);
      expect(e.title).toBe(`error.${e.code}.title`);
      expect(e.detail).toBe(`error.${e.code}.detail`);
      expect(e.actions.length).toBeGreaterThan(0);
      codes.push(e.code);
    }
    expect(codes).toEqual(["cloud_invalid_signature", "cloud_invalid_token", "cloud_trial_expired", "cloud_permission_denied", "cloud_wrong_region"]);
  });

  it("la región equivocada sugiere la correcta", async () => {
    installMocks({ scenario: "fresh", fast: true });
    const e = await commands.cloudValidate(creds({ region: "eu" })).catch((x) => x);
    expect(e.params.region).toBe("us");
    await expect(commands.cloudValidate(creds())).resolves.toMatchObject({ region: "us" });
  });

  it("valida los ajustes contra el catálogo", async () => {
    installMocks({ scenario: "configured", fast: true });
    const e = await commands.settingsSet({ kind: "global" }, "saturation", 9).catch((x) => x);
    expect(e.code).toBe("invalid_setting");
    const s = await commands.settingsSet({ kind: "global" }, "saturation", 1.5);
    expect(s.global.saturation).toBe(1.5);
    const r = await commands.settingsReset({ kind: "global" }, "saturation");
    expect(r.global.saturation).toBe(1.3);
  });

  it("el 914 se resuelve con renovar clave", async () => {
    installMocks({ scenario: "configured", fast: true });
    const [, desk] = await commands.devicesList();
    const e = await commands.deviceTest(desk.id, "red_green_blue").catch((x) => x);
    expect(e.code).toBe("device_key_rejected");
    expect(e.actions[0]).toEqual({ kind: "renew_key" });
    await commands.renewKey(desk.id);
    await expect(commands.deviceTest(desk.id, "red_green_blue")).resolves.toBeNull();
  });

  it("guarda el progreso del asistente", async () => {
    installMocks({ scenario: "fresh", fast: true });
    await commands.onboardingSave("credentials", ["welcome", "permissions"]);
    const s = await commands.onboardingState();
    expect(s.step).toBe("credentials");
    expect(s.completed).toEqual(["welcome", "permissions"]);
  });

  it("la calibración respeta los rangos de calibrar.py", async () => {
    installMocks({ scenario: "configured", fast: true });
    const [living] = await commands.devicesList();
    const quick = await commands.calibrationStart(living.id, "1", true);
    expect(quick.points.map((p) => p.screen_hue_deg)).toEqual([0, 60, 120, 180, 240, 300]);
    const s = await commands.calibrationSet(1, 400, 3);
    expect(s.points[1].tono_deg).toBe(150);
    expect(s.points[1].sat).toBe(1.5);
  });
});
