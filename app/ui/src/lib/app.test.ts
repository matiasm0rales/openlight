// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import { app, raise, runFix } from "./app.svelte";
import { err } from "./mocks/backend";

afterEach(() => {
  clearMocks();
  app.errors = [];
});

describe("acciones de los errores", () => {
  it("un reintento que vuelve a fallar deja el error visible, con su botón", async () => {
    const error = err("cloud_network", [{ kind: "retry" }]);
    const failing = () => raise(error, failing);
    raise(error, failing);
    const shown = app.errors[0];
    const solved = await runFix({ kind: "retry" }, shown, failing);
    expect(solved).toBe(false);
    expect(app.errors).toHaveLength(1);
    expect(app.errors[0].uid).not.toBe(shown.uid);
    expect(app.errors[0].retry).toBe(failing);
  });

  it("un reintento que tira una excepción también queda a la vista", async () => {
    const error = err("storage", [{ kind: "retry" }]);
    const throwing = () => Promise.reject(err("storage", [{ kind: "retry" }]));
    raise(error, throwing);
    expect(await runFix({ kind: "retry" }, app.errors[0], throwing)).toBe(false);
    expect(app.errors.map((e) => e.code)).toEqual(["storage"]);
  });

  it("un reintento que anda saca el error", async () => {
    raise(err("cloud_network", [{ kind: "retry" }]));
    expect(await runFix({ kind: "retry" }, app.errors[0], () => undefined)).toBe(false);
    expect(app.errors).toEqual([]);
  });
});
