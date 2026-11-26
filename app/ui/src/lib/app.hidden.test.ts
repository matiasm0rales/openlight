// @vitest-environment jsdom
// Con la ventana en el tray, minimizada o tapada, los eventos en vivo no mueven el estado.
import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { app, init, MAX_ERRORS, raise } from "./app.svelte";
import { EVENT_COLOR_LIVE, EVENT_DEVICE_HEALTH, EVENT_PIPELINE_STATS, EVENT_SESSION_STATE } from "./bindings";
import { err, installMocks } from "./mocks/backend";
import { setHidden } from "./testing/page";

const live = (luz: number) => ({ device: "foco", screen: { h: null, s: 0, luz }, sent: null });
const health = (bytes: number) => ({
  device: "foco",
  state: { kind: "active" as const },
  health: { connected: true, rtt_ms: 5, reconnects: 0, disconnects_last_hour: 0, last_disconnect: null, last_disconnect_at_ms: null, bytes_rx: bytes, bytes_tx: bytes },
});
const stats = { capture_fps: 20, devices: [{ device: "foco", sends_per_s: 8 }] };

beforeAll(async () => {
  installMocks({ scenario: "configured", fast: true });
  await init();
});
afterAll(() => clearMocks());
afterEach(() => {
  vi.useRealTimers();
  setHidden(false);
  app.live = {};
  app.health = {};
  app.stats = [];
  app.errors = [];
  app.session = "idle";
});

describe("página oculta", () => {
  it("los eventos en vivo no tocan el estado y al volver se aplica el último", async () => {
    setHidden(true);
    await emit(EVENT_COLOR_LIVE, live(0.1));
    await emit(EVENT_COLOR_LIVE, live(0.9));
    await emit(EVENT_DEVICE_HEALTH, health(1));
    await emit(EVENT_DEVICE_HEALTH, health(2));
    expect(app.live).toEqual({});
    expect(app.health).toEqual({});
    setHidden(false);
    expect(app.live.foco.screen.luz).toBe(0.9);
    expect(app.health.foco.health.bytes_rx).toBe(2);
  });

  it("lo que llega a la vista gana sobre lo guardado", async () => {
    setHidden(true);
    await emit(EVENT_COLOR_LIVE, live(0.1));
    setHidden(false);
    await emit(EVENT_COLOR_LIVE, live(0.5));
    setHidden(true);
    setHidden(false);
    expect(app.live.foco.screen.luz).toBe(0.5);
  });

  it("no junta muestras de estadísticas", async () => {
    setHidden(true);
    await emit(EVENT_PIPELINE_STATS, stats);
    setHidden(false);
    expect(app.stats).toEqual([]);
    await emit(EVENT_PIPELINE_STATS, stats);
    expect(app.stats).toHaveLength(1);
  });

  // El gráfico dibuja las muestras una al lado de la otra: las de antes del hueco parecerían de recién
  const later = (ms: number) => vi.setSystemTime(Date.now() + ms);

  it("al volver después de un rato no muestra estadísticas viejas", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    await emit(EVENT_PIPELINE_STATS, stats);
    setHidden(true);
    later(90_000);
    setHidden(false);
    expect(app.stats).toEqual([]);
  });

  it("tras un hueco sin muestras arranca el historial de nuevo", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    await emit(EVENT_PIPELINE_STATS, stats);
    later(1000);
    await emit(EVENT_PIPELINE_STATS, stats);
    expect(app.stats).toHaveLength(2);
    // Sin cambio de visibilidad: el backend dejó de mandarlas un rato
    later(10_000);
    await emit(EVENT_PIPELINE_STATS, stats);
    expect(app.stats).toHaveLength(1);
  });

  it("una ocultada corta conserva el historial", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    await emit(EVENT_PIPELINE_STATS, stats);
    setHidden(true);
    later(1500);
    setHidden(false);
    await emit(EVENT_PIPELINE_STATS, stats);
    expect(app.stats).toHaveLength(2);
  });

  it("el estado de la sesión se sigue igual", async () => {
    setHidden(true);
    await emit(EVENT_SESSION_STATE, { state: "active" });
    expect(app.session).toBe("active");
  });
});

describe("listas con límite", () => {
  it("las estadísticas guardan un minuto", async () => {
    for (let i = 0; i < 70; i++) await emit(EVENT_PIPELINE_STATS, stats);
    expect(app.stats).toHaveLength(60);
  });

  it("la pila de errores descarta los más viejos", () => {
    for (let i = 0; i < MAX_ERRORS + 5; i++) raise(err("storage", [{ kind: "retry" }], { cause: String(i) }));
    expect(app.errors).toHaveLength(MAX_ERRORS);
    expect(app.errors.at(-1)?.params.cause).toBe(String(MAX_ERRORS + 4));
  });
});
