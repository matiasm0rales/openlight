// @vitest-environment jsdom
// El orden de los focos en la UI: se ve al instante, se guarda y vuelve atrás si falla.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";
import { app, refreshDevices, reorderDevices } from "./app.svelte";
import { err, installMocks, type MockBackend } from "./mocks/backend";

let backend: MockBackend;
/** Los órdenes que le llegaron al backend. */
let sent: string[][];
/** Cuántos de los próximos guardados fallan, contando desde el que se indique. */
let failFrom: number;

const shown = () => app.devices.map((d) => d.name);
const stored = () => backend.state.devices.map((d) => d.name);
const order = (...names: string[]) => names.map((n) => app.devices.find((d) => d.name === `Foco ${n}` || d.name === n)!.id);
const A = "Foco living";
const B = "Lámpara escritorio";
const C = "Foco repisa";
const D = "Foco techo";

beforeEach(async () => {
  backend = installMocks({ scenario: "four", fast: true });
  sent = [];
  failFrom = Infinity;
  const handle = backend.handle.bind(backend);
  backend.handle = async (cmd, args) => {
    if (cmd === "devices_reorder") {
      sent.push(args.ids);
      if (sent.length >= failFrom) {
        await backend.wait(10);
        throw err("storage", [{ kind: "retry" }]);
      }
    }
    return handle(cmd, args);
  };
  app.errors = [];
  await refreshDevices();
});

afterEach(() => clearMocks());

describe("reordenar los focos", () => {
  it("la lista cambia ya y queda guardada", async () => {
    const done = reorderDevices(order(B, A, C, D));
    expect(shown()).toEqual([B, A, C, D]);
    expect(await done).toBe(true);
    expect(stored()).toEqual([B, A, C, D]);
    expect(shown()).toEqual([B, A, C, D]);
    expect(app.errors).toEqual([]);
  });

  it("si no se puede guardar vuelve a como estaba y lo dice", async () => {
    failFrom = 1;
    const done = reorderDevices(order(D, A, B, C));
    expect(shown()).toEqual([D, A, B, C]);
    expect(await done).toBe(false);
    expect(shown()).toEqual([A, B, C, D]);
    expect(app.errors.map((e) => e.code)).toEqual(["storage"]);
  });

  it("«Reintentar» vuelve a guardar el orden que había quedado afuera", async () => {
    failFrom = 1;
    await reorderDevices(order(B, A, C, D));
    failFrom = Infinity;
    await app.errors[0].retry!();
    expect(stored()).toEqual([B, A, C, D]);
    expect(shown()).toEqual([B, A, C, D]);
  });

  it("varios cambios seguidos no se pisan: queda el último", async () => {
    const first = reorderDevices(order(B, A, C, D));
    void reorderDevices(order(B, C, A, D));
    const last = reorderDevices(order(B, C, D, A));
    expect(shown()).toEqual([B, C, D, A]);
    expect(await Promise.all([first, last])).toEqual([true, true]);
    expect(stored()).toEqual([B, C, D, A]);
    expect(sent).toHaveLength(2); // el del medio ya no hacía falta
  });

  it("si falla uno posterior, vuelve al último que sí se guardó", async () => {
    failFrom = 2;
    void reorderDevices(order(B, A, C, D));
    const last = reorderDevices(order(B, C, A, D));
    expect(await last).toBe(false);
    expect(shown()).toEqual([B, A, C, D]);
    expect(stored()).toEqual([B, A, C, D]);
  });

  it("releer la lista mientras se guarda no deshace lo que se ve", async () => {
    const done = reorderDevices(order(D, C, B, A));
    await refreshDevices();
    expect(shown()).toEqual([D, C, B, A]);
    await done;
  });

  it("si la lista de focos cambió, muestra la del backend y no ofrece repetir lo mismo", async () => {
    backend.state.devices = backend.state.devices.filter((d) => d.name !== D);
    expect(await reorderDevices(order(B, A, C, D))).toBe(false);
    expect(app.errors.map((e) => e.code)).toEqual(["device_order_mismatch"]);
    expect(app.errors[0].retry).toBeUndefined();
    await expect.poll(shown).toEqual([A, B, C]);
  });
});
