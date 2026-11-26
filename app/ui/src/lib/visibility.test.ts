// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setHidden } from "./testing/page";
import { everyWhileVisible } from "./visibility";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  setHidden(false);
  vi.useRealTimers();
});

describe("repetir solo con la página a la vista", () => {
  it("corre cada tanto mientras se ve y deja de correr oculta", () => {
    const run = vi.fn();
    const stop = everyWhileVisible(run, 1000);
    vi.advanceTimersByTime(2000);
    expect(run).toHaveBeenCalledTimes(2);
    setHidden(true);
    vi.advanceTimersByTime(10_000);
    expect(run).toHaveBeenCalledTimes(2);
    stop();
  });

  it("al volver corre enseguida y sigue", () => {
    const run = vi.fn();
    const stop = everyWhileVisible(run, 1000);
    setHidden(true);
    setHidden(false);
    expect(run).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1000);
    expect(run).toHaveBeenCalledTimes(2);
    stop();
  });

  it("si nace oculta no corre hasta verse", () => {
    setHidden(true);
    const run = vi.fn();
    const stop = everyWhileVisible(run, 1000);
    vi.advanceTimersByTime(5000);
    expect(run).not.toHaveBeenCalled();
    setHidden(false);
    expect(run).toHaveBeenCalledTimes(1);
    stop();
  });

  it("parada, no vuelve ni al cambiar la visibilidad", () => {
    const run = vi.fn();
    everyWhileVisible(run, 1000)();
    setHidden(true);
    setHidden(false);
    vi.advanceTimersByTime(5000);
    expect(run).not.toHaveBeenCalled();
  });
});
