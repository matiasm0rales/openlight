// @vitest-environment jsdom
// «Reparar permiso»: el de Grabación de pantalla que macOS dejó atado a otra versión de la app.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import Permissions from "../screens/onboarding/Permissions.svelte";
import { app, raise, runFix } from "./app.svelte";
import ErrorStack from "./components/ErrorStack.svelte";
import { setLocale } from "./i18n/index.svelte";
import { err, installMocks } from "./mocks/backend";

setLocale("es");

afterEach(() => {
  cleanup();
  clearMocks();
  app.errors = [];
  app.screenRepaired = false;
});

const repair = { kind: "repair_screen_permission" as const };

/** Backend que anota los comandos; `answers` decide qué contesta cada uno. */
function backend(answers: Record<string, () => unknown> = {}): string[] {
  const calls: string[] = [];
  mockIPC((cmd) => {
    calls.push(cmd);
    return answers[cmd]?.() ?? null;
  });
  return calls;
}

const screenPermission = (state: "denied" | "not_determined" | "granted") => () => [
  { kind: "screen_recording", state, pane: "screen_recording", needs_restart: state !== "granted" },
];

describe("reparar el permiso de pantalla", () => {
  it("llama al backend, saca los avisos del permiso y explica que hay que reabrir", async () => {
    const calls = backend();
    raise(err("screen_permission_stale", [repair]));
    raise(err("screen_permission_denied", [repair]));
    raise(err("no_zones", []));
    expect(await runFix(repair, app.errors[0])).toBe(true);
    expect(calls).toEqual(["repair_screen_permission"]);
    expect(app.errors.map((e) => e.code)).toEqual(["no_zones"]);
    expect(app.screenRepaired).toBe(true);
  });

  it("si falla, el aviso original sigue y se suma el motivo", async () => {
    backend({
      repair_screen_permission: () => {
        throw err("internal", [{ kind: "open_settings", value: "screen_recording" }], { detail: "tccutil" });
      },
    });
    raise(err("screen_permission_stale", [repair]));
    expect(await runFix(repair, app.errors[0])).toBe(false);
    expect(app.errors.map((e) => e.code)).toEqual(["screen_permission_stale", "internal"]);
    expect(app.screenRepaired).toBe(false);
  });

  it("el error ofrece el botón y después queda el aviso para reabrir, con acceso a Ajustes", async () => {
    const calls = backend();
    raise(err("screen_permission_stale", [repair, { kind: "open_settings", value: "screen_recording" }]));
    render(ErrorStack);
    await fireEvent.click(screen.getByRole("button", { name: "Reparar permiso" }));
    await screen.findByText("Permiso de pantalla reiniciado");
    expect(screen.getByText(/volvé a abrirlo/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Reparar permiso" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Abrir Grabación de pantalla" }));
    await waitFor(() => expect(calls).toEqual(["repair_screen_permission", "open_system_settings"]));
  });

  it("en el asistente aparece cuando el permiso figura denegado", async () => {
    const calls = backend({ check_permissions: screenPermission("denied") });
    render(Permissions, { valid: false });
    await fireEvent.click(await screen.findByRole("button", { name: "Reparar permiso" }));
    await waitFor(() => expect(calls).toContain("repair_screen_permission"));
    await waitFor(() => expect(app.screenRepaired).toBe(true));
  });

  it("y no confunde a quien todavía no lo pidió ni a quien ya lo tiene", async () => {
    for (const state of ["not_determined", "granted"] as const) {
      backend({ check_permissions: screenPermission(state) });
      render(Permissions, { valid: false });
      await screen.findByText("Grabación de pantalla");
      expect(screen.queryByRole("button", { name: "Reparar permiso" })).toBeNull();
      cleanup();
      clearMocks();
    }
  });
});

describe("backend falso: permiso de pantalla", () => {
  it("sin permiso ofrece abrir Ajustes y reparar, como la app en macOS", async () => {
    installMocks({ scenario: "configured", fast: true });
    const { commands } = await import("./bindings");
    await commands.repairScreenPermission();
    const e = await commands.sessionStart().catch((x) => x);
    expect(e.code).toBe("screen_permission_denied");
    expect(e.detail).toBe("error.screen_permission_denied.detail_macos");
    expect(e.actions).toEqual([{ kind: "open_settings", value: "screen_recording" }, repair]);
  });
});
