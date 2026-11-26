// @vitest-environment jsdom
// La ventana principal se destruye al cerrarla: cada vez que se abre, la página arranca de cero y
// le pide al backend lo que pasó mientras no estaba.
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  clearMocks();
  location.hash = "";
});

/** Una ventana nueva sobre un backend que ya venía con el efecto andando. */
async function open(hash: string) {
  vi.resetModules();
  localStorage.clear();
  location.hash = hash;
  const { installMocks } = await import("./mocks/backend");
  const backend = installMocks({ scenario: "configured", fast: true });
  await backend.start();
  // Sin los relojes del mock: lo único que puede llegar es lo que se pide
  backend.stopTimers();
  const { app, init, repairScreenPermission } = await import("./app.svelte");
  await init();
  return { backend, app, repair: repairScreenPermission };
}

describe("ventana nueva", () => {
  it("recibe el estado completo de los focos", async () => {
    const { backend, app } = await open("");
    expect(app.session).toBe("active");
    expect(Object.keys(app.health).sort()).toEqual(backend.state.devices.map((d) => d.id).sort());
  });

  it("el aviso de reabrir la app sigue en la ventana nueva hasta que se lo descarta", async () => {
    const first = await open("");
    await first.repair();
    /** La ventana se cierra y se abre otra: página nueva, el mismo backend. */
    const reopen = async () => {
      vi.resetModules();
      const page = await import("./app.svelte");
      await page.init();
      return page;
    };
    let page = await reopen();
    expect(page.app.screenRepaired).toBe(true);
    await page.dismissRepairNotice();
    expect(page.app.screenRepaired).toBe(false);
    page = await reopen();
    expect(page.app.screenRepaired).toBe(false);
  });

  it("la de calibración no se lleva lo que es de la principal", async () => {
    const { app } = await open("#/calibrate");
    expect(app.health).toEqual({});
  });
});
