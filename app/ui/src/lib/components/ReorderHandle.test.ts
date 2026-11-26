// @vitest-environment jsdom
// Lo que no se puede provocar desde el navegador: que un arrastre se corte por fuera.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { setLocale } from "../i18n/index.svelte";
import ReorderHandle from "./ReorderHandle.svelte";

setLocale("es");
afterEach(cleanup);

function handle() {
  const on = { onmove: vi.fn(), ondrag: vi.fn(), ondrop: vi.fn(), oncancel: vi.fn() };
  const view = render(ReorderHandle, { id: "a", name: "Foco", first: false, last: false, describedby: "ayuda", ...on });
  const grip = screen.getByRole("button", { name: "Reordenar Foco" });
  // jsdom no captura punteros.
  grip.setPointerCapture = () => {};
  return { ...view, grip, on };
}

/** Un puntero principal: jsdom no trae `PointerEvent`. */
function pointer(target: Element, type: string, x: number, y: number) {
  const event = new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 });
  Object.assign(event, { pointerId: 1, isPrimary: true });
  return fireEvent(target, event);
}

async function dragging() {
  const h = handle();
  await pointer(h.grip, "pointerdown", 10, 200);
  await pointer(h.grip, "pointermove", 10, 260);
  expect(h.on.ondrag).toHaveBeenLastCalledWith(10, 260);
  return h;
}

describe("asa para reordenar", () => {
  it("un temblor del pulso no es un arrastre", async () => {
    const { grip, on } = handle();
    await pointer(grip, "pointerdown", 10, 200);
    await pointer(grip, "pointermove", 12, 203);
    await pointer(grip, "pointerup", 12, 203);
    expect(on.ondrag).not.toHaveBeenCalled();
    expect(on.ondrop).not.toHaveBeenCalled();
  });

  it("si el sistema le saca el puntero, el arrastre se cancela", async () => {
    const { grip, on } = await dragging();
    await pointer(grip, "pointercancel", 10, 260);
    await pointer(grip, "pointerup", 10, 260);
    expect(on.oncancel).toHaveBeenCalledTimes(1);
    expect(on.ondrop).not.toHaveBeenCalled();
  });

  it("si el asa desaparece a mitad de un arrastre, lo cancela", async () => {
    const { unmount, on } = await dragging();
    unmount();
    expect(on.oncancel).toHaveBeenCalledTimes(1);
  });

  it("desaparecer sin estar arrastrando no cancela nada", () => {
    const { unmount, on } = handle();
    unmount();
    expect(on.oncancel).not.toHaveBeenCalled();
  });
});
