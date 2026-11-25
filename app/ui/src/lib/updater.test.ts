// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import type { UpdateScenario } from "./mocks/updater";

const HOUR = 60 * 60 * 1000;

/** Una app recién abierta: módulos nuevos (el estado del updater es global) y backend falso. */
async function boot(update: UpdateScenario, stored: Record<string, string> = {}) {
  vi.resetModules();
  localStorage.clear();
  for (const [k, v] of Object.entries(stored)) localStorage.setItem(k, v);
  const { installMocks } = await import("./mocks/backend");
  const backend = installMocks({ scenario: "configured", fast: true, update });
  const updater = await import("./updater.svelte");
  const { app } = await import("./app.svelte");
  return { backend, mock: backend.updater, app, ...updater };
}

const asked = (calls: string[], what: string) => calls.filter((c) => c === what).length;

beforeEach(() => vi.spyOn(console, "warn").mockImplementation(() => {}));
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  clearMocks();
});

describe("buscar actualizaciones", () => {
  it("encuentra la versión nueva con sus notas", async () => {
    const u = await boot("available");
    await u.checkForUpdates();
    expect(u.updates.available).toEqual({ version: "0.2.0", current: "0.1.0", notes: expect.stringContaining("• Escenas en el control manual.") });
    expect(u.updates.upToDate).toBe(false);
    expect(u.updates.phase).toBe("idle");
  });

  it("sin versión nueva dice que está al día", async () => {
    const u = await boot("none");
    await u.checkForUpdates(true);
    expect(u.updates.available).toBeNull();
    expect(u.updates.upToDate).toBe(true);
  });

  it("sin red: la búsqueda automática no molesta y la manual muestra el error", async () => {
    const u = await boot("offline");
    await u.checkForUpdates();
    expect(u.updates.error).toBeNull();
    await u.checkForUpdates(true);
    expect(u.updates.error).toMatchObject({ step: "check", kind: "network" });
    expect(u.updates.upToDate).toBe(false);
    // Vuelve la red: el error se va.
    u.mock.scenario = "available";
    await u.checkForUpdates(true);
    expect(u.updates.error).toBeNull();
    expect(u.updates.available?.version).toBe("0.2.0");
  });

  it("no encima dos búsquedas", async () => {
    const u = await boot("available");
    await Promise.all([u.checkForUpdates(), u.checkForUpdates()]);
    expect(asked(u.mock.calls, "check")).toBe(1);
  });
});

describe("búsqueda automática", () => {
  it("está prendida por defecto: busca al arrancar y cada 6 horas", async () => {
    vi.useFakeTimers();
    const u = await boot("none");
    expect(u.updates.auto).toBe(true);
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(1000);
    expect(asked(u.mock.calls, "check")).toBe(1);
    expect(u.updates.version).toBe("0.1.0");
    await vi.advanceTimersByTimeAsync(5 * HOUR);
    expect(asked(u.mock.calls, "check")).toBe(1);
    await vi.advanceTimersByTimeAsync(2 * HOUR);
    expect(asked(u.mock.calls, "check")).toBe(2);
    stop();
    await vi.advanceTimersByTimeAsync(24 * HOUR);
    expect(asked(u.mock.calls, "check")).toBe(2);
  });

  it("si falló, reintenta antes de las 6 horas", async () => {
    vi.useFakeTimers();
    const u = await boot("offline");
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(31 * 60 * 1000);
    expect(asked(u.mock.calls, "check")).toBeGreaterThanOrEqual(3);
    stop();
  });

  it("al volver a mostrar la ventana busca si pasó el plazo (la compu estuvo suspendida)", async () => {
    vi.useFakeTimers();
    const u = await boot("none");
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(1000);
    document.dispatchEvent(new Event("visibilitychange"));
    expect(asked(u.mock.calls, "check")).toBe(1);
    vi.setSystemTime(Date.now() + 8 * HOUR);
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(1000);
    expect(asked(u.mock.calls, "check")).toBe(2);
    stop();
  });

  it("apagada no busca sola, se recuerda y al prenderla busca enseguida", async () => {
    vi.useFakeTimers();
    const u = await boot("available", { "openlight.updates.auto": "off" });
    expect(u.updates.auto).toBe(false);
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(13 * HOUR);
    expect(asked(u.mock.calls, "check")).toBe(0);
    u.setAutoCheck(true);
    await vi.advanceTimersByTimeAsync(1000);
    expect(asked(u.mock.calls, "check")).toBe(1);
    expect(localStorage.getItem("openlight.updates.auto")).toBe("on");
    u.setAutoCheck(false);
    expect(localStorage.getItem("openlight.updates.auto")).toBe("off");
    stop();
  });
});

describe("actualizar y reiniciar", () => {
  it("descarga, devuelve los focos, instala y reinicia, en ese orden", async () => {
    const u = await boot("available");
    await u.checkForUpdates();
    await u.installUpdate();
    expect(u.mock.calls).toEqual(["check", "download", "update_prepare", "install", "restart"]);
    expect(u.mock.restarted).toBe(true);
    expect(u.updates.phase).toBe("restarting");
    expect(u.updates.error).toBeNull();
  });

  it("muestra el progreso de la descarga", async () => {
    const u = await boot("hang");
    await u.checkForUpdates();
    const done = u.installUpdate();
    await vi.waitFor(() => expect(u.updates.progress.done).toBeGreaterThan(0));
    await vi.waitFor(() => expect(u.updates.progress.done / u.updates.progress.total!).toBeCloseTo(0.4));
    expect(u.updates.phase).toBe("downloading");
    // Mientras descarga no se puede lanzar otra instalación ni una búsqueda.
    await u.installUpdate();
    await u.checkForUpdates(true);
    expect(asked(u.mock.calls, "download")).toBe(1);
    expect(asked(u.mock.calls, "check")).toBe(1);
    u.mock.finishDownload();
    await done;
    expect(u.updates.progress.done).toBe(u.updates.progress.total);
  });

  for (const [scenario, kind] of [
    ["bad_signature", "signature"],
    ["offline_download", "network"],
  ] as const) {
    it(`si la descarga falla (${scenario}) no toca los focos y se puede reintentar`, async () => {
      const u = await boot(scenario);
      await u.checkForUpdates();
      await u.installUpdate();
      expect(u.updates.error).toMatchObject({ step: "download", kind });
      expect(u.updates.phase).toBe("idle");
      expect(u.mock.calls).not.toContain("update_prepare");
      expect(u.mock.calls).not.toContain("install");
      u.mock.scenario = "available";
      await u.installUpdate();
      expect(u.updates.error).toBeNull();
      expect(u.mock.restarted).toBe(true);
    });
  }

  it("si instalar falla, la app sigue andando y el efecto vuelve a arrancar", async () => {
    const u = await boot("install_fails");
    await u.backend.start();
    await u.checkForUpdates();
    await u.installUpdate();
    expect(u.updates.error).toMatchObject({ step: "install", kind: "permission" });
    expect(u.mock.calls.slice(-3)).toEqual(["install", "update_abort", "session_start"]);
    expect(u.backend.session).toBe("active");
    await u.backend.stop();
  });

  it("si instalar falla con el efecto apagado, no lo prende", async () => {
    const u = await boot("install_fails");
    await u.checkForUpdates();
    await u.installUpdate();
    expect(u.mock.calls.at(-1)).toBe("update_abort");
    expect(u.backend.session).toBe("idle");
  });

  it("si no se puede reiniciar, queda instalada y ofrece reiniciar de nuevo", async () => {
    const u = await boot("restart_fails");
    await u.checkForUpdates();
    await u.installUpdate();
    expect(u.updates.error).toMatchObject({ step: "install", kind: "restart" });
    expect(u.updates.installed).toBe(true);
    expect(u.mock.calls.at(-1)).toBe("update_abort");
    // Ya instalada: "Actualizar" no vuelve a descargar.
    await u.installUpdate();
    expect(asked(u.mock.calls, "download")).toBe(1);
    u.mock.scenario = "available";
    await u.restartApp();
    expect(u.mock.calls.slice(-2)).toEqual(["update_prepare", "restart"]);
    expect(u.mock.restarted).toBe(true);
  });

  it("«Más tarde» esconde el aviso de esa versión hasta que se vuelve a buscar a mano", async () => {
    const u = await boot("available");
    await u.checkForUpdates();
    expect(u.noticeVisible()).toBe(true);
    u.dismissUpdate();
    expect(u.noticeVisible()).toBe(false);
    await u.checkForUpdates();
    expect(u.noticeVisible()).toBe(false);
    await u.checkForUpdates(true);
    expect(u.noticeVisible()).toBe(true);
  });

  it("descartar un error de instalación también esconde el aviso", async () => {
    const u = await boot("bad_signature");
    await u.checkForUpdates();
    await u.installUpdate();
    u.dismissUpdate();
    expect(u.updates.error).toBeNull();
    expect(u.noticeVisible()).toBe(false);
  });
});
