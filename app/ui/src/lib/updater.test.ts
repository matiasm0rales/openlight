// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import type { UpdateScenario } from "./mocks/updater";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;

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
/** macOS vuelve a mostrar la ventana. */
const flap = () => document.dispatchEvent(new Event("visibilitychange"));

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

  it("sin ninguna versión publicada (404) está al día, sin error", async () => {
    const u = await boot("no_release");
    for (const manual of [false, true]) {
      await u.checkForUpdates(manual);
      expect(u.updates.upToDate).toBe(true);
      expect(u.updates.error).toBeNull();
      expect(u.updates.available).toBeNull();
    }
    expect(console.warn).not.toHaveBeenCalled();
  });

  it("si el servidor deja de contestar, la versión ya ofrecida no se pierde", async () => {
    const u = await boot("available");
    await u.checkForUpdates();
    u.mock.scenario = "no_release";
    await u.checkForUpdates();
    expect(u.updates.available?.version).toBe("0.2.0");
    expect(u.updates.upToDate).toBe(false);
    await u.checkForUpdates(true);
    expect(u.updates.error).toMatchObject({ step: "check", kind: "unavailable" });
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

  // La app instalada hizo unas 330 búsquedas en 4 minutos: macOS marca la ventana como tapada y
  // vuelve a mostrarla varias veces por segundo, y cada vuelta buscaba si la anterior había fallado.
  for (const [scenario, expected] of [
    ["none", 1],
    ["available", 1],
    ["no_release", 1],
    ["offline", 3], // al arrancar y los reintentos de 1 y 2 minutos
  ] as const) {
    it(`una ventana que se tapa y se destapa sin parar no dispara búsquedas (${scenario})`, async () => {
      vi.useFakeTimers();
      const u = await boot(scenario);
      const stop = u.startUpdater();
      for (let i = 0; i < 4 * MINUTE; i += 250) {
        await vi.advanceTimersByTimeAsync(250);
        flap();
      }
      expect(asked(u.mock.calls, "check")).toBe(expected);
      stop();
    });
  }

  it("sin red reintenta con esperas que se duplican, hasta una hora", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    const u = await boot("offline");
    const minutes: number[] = [];
    const check = u.mock.check.bind(u.mock);
    u.mock.check = () => (minutes.push(Math.floor(Date.now() / MINUTE)), check());
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(3 * HOUR + 4 * MINUTE);
    const waits = minutes.slice(1).map((t, i) => t - minutes[i]);
    expect(waits).toEqual([1, 2, 4, 8, 16, 32, 60, 60]);
    expect(u.updates.error).toBeNull();
    // Vuelve la red: se encuentra la versión y se espera otra vez 6 horas.
    u.mock.scenario = "available";
    await vi.advanceTimersByTimeAsync(HOUR);
    expect(u.updates.available?.version).toBe("0.2.0");
    const found = asked(u.mock.calls, "check");
    await vi.advanceTimersByTimeAsync(5 * HOUR);
    expect(asked(u.mock.calls, "check")).toBe(found);
    // Si vuelve a fallar, empieza de nuevo por la espera corta.
    u.mock.scenario = "offline";
    await vi.advanceTimersByTimeAsync(HOUR + 2 * MINUTE);
    expect(minutes[found + 1] - minutes[found]).toBe(1);
    stop();
  });

  it("volver a mostrar la ventana no busca si la última búsqueda fue hace menos de una hora", async () => {
    vi.useFakeTimers();
    const u = await boot("offline");
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(1000);
    // El reloj salta sin que corran los temporizadores: así se ve solo lo que dispara la ventana.
    vi.setSystemTime(Date.now() + 30 * MINUTE);
    flap();
    expect(asked(u.mock.calls, "check")).toBe(1);
    vi.setSystemTime(Date.now() + 31 * MINUTE);
    flap();
    expect(asked(u.mock.calls, "check")).toBe(2);
    stop();
  });

  it("nunca hay dos búsquedas a la vez", async () => {
    vi.useFakeTimers();
    const u = await boot("none");
    let finish = () => {};
    const gate = new Promise<void>((r) => (finish = r));
    const check = u.mock.check.bind(u.mock);
    u.mock.check = () => check().then(async (found) => (await gate, found));
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(8 * HOUR); // la primera sigue sin contestar
    flap();
    u.setAutoCheck(true);
    await u.checkForUpdates(true);
    expect(asked(u.mock.calls, "check")).toBe(1);
    finish();
    await vi.advanceTimersByTimeAsync(1000);
    expect(u.updates.phase).toBe("idle");
    stop();
  });

  it("volver a arrancar el updater no busca de nuevo", async () => {
    vi.useFakeTimers();
    const u = await boot("none");
    u.startUpdater()();
    await vi.advanceTimersByTimeAsync(1000);
    const stop = u.startUpdater();
    await vi.advanceTimersByTimeAsync(1000);
    expect(asked(u.mock.calls, "check")).toBe(1);
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
