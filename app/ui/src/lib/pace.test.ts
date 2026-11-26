// @vitest-environment jsdom
// El color en vivo llega por foco hasta 10 veces por segundo: se dibuja todo junto, de a un cuadro.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CALM_FRAME_MS, LIVE_FRAME_MS, frameMs, pacedByDevice } from "./pace";
import { setHidden } from "./testing/page";

interface Color {
  device: string;
  luz: number;
}

/** Lo que se aplicó en cada tanda. */
function recorder() {
  const batches: string[][] = [];
  const push = pacedByDevice<Color>((items) => batches.push([...items].map((c) => `${c.device}:${c.luz}`)), () => LIVE_FRAME_MS);
  return { batches, push: (device: string, luz: number) => push({ device, luz }) };
}

/** Pasa el tiempo, incluido el próximo cuadro de dibujo. */
const pass = (ms: number) => vi.advanceTimersByTimeAsync(ms);
/** Margen para el cuadro de dibujo que sigue a la espera. */
const FRAME = 20;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  setHidden(false);
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("color en vivo a un ritmo fijo", () => {
  it("lo primero se dibuja enseguida", async () => {
    const r = recorder();
    r.push("a", 1);
    await pass(FRAME);
    expect(r.batches).toEqual([["a:1"]]);
  });

  it("lo que llega antes del próximo cuadro se junta, con lo último de cada foco", async () => {
    const r = recorder();
    r.push("a", 1);
    await pass(FRAME);
    r.push("a", 2);
    await pass(30);
    r.push("b", 3);
    r.push("a", 4);
    expect(r.batches).toHaveLength(1);
    await pass(LIVE_FRAME_MS);
    expect(r.batches[1]).toEqual(["a:4", "b:3"]);
  });

  it("dos focos desfasados no pasan de una tanda por intervalo", async () => {
    const r = recorder();
    for (let t = 0; t < 1000; t += 50) {
      r.push(t % 100 ? "b" : "a", t);
      await pass(50);
    }
    expect(r.batches.length).toBeGreaterThanOrEqual(8);
    expect(r.batches.length).toBeLessThanOrEqual(1000 / LIVE_FRAME_MS + 1);
  });

  it("oculta no aplica nada, y al volver aplica lo último", async () => {
    const r = recorder();
    setHidden(true);
    r.push("a", 1);
    r.push("a", 2);
    await pass(1000);
    expect(r.batches).toEqual([]);
    setHidden(false);
    expect(r.batches).toEqual([["a:2"]]);
    // Y sigue andando
    await pass(LIVE_FRAME_MS);
    r.push("a", 3);
    await pass(FRAME);
    expect(r.batches.at(-1)).toEqual(["a:3"]);
  });

  it("con «reducir movimiento» el color cambia menos seguido", () => {
    expect(frameMs()).toBe(LIVE_FRAME_MS);
    vi.stubGlobal("matchMedia", (query: string) => ({ matches: query === "(prefers-reduced-motion: reduce)" }));
    expect(frameMs()).toBe(CALM_FRAME_MS);
  });
});
