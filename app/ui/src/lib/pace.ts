// El color en vivo llega por foco hasta 10 veces por segundo y cada cambio redibuja la ventana.
// Acá se junta lo de todos los focos y se aplica de una vez, al ritmo del dibujo.
import { pageHidden, whenVisible } from "./visibility";

/** Cada cuánto se redibuja como mucho el color en vivo. */
export const LIVE_FRAME_MS = 100;
/** Lo mismo para quien pidió "reducir movimiento" en el sistema. */
export const CALM_FRAME_MS = 500;

export function frameMs(): number {
  const calm = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  return calm ? CALM_FRAME_MS : LIVE_FRAME_MS;
}

/**
 * Guarda lo último de cada foco y se lo pasa a `apply` de una vez: como mucho una tanda cada
 * `interval()` y justo antes de un cuadro de dibujo. Con la página oculta no aplica nada; al
 * volver a verla aplica lo último enseguida.
 */
export function pacedByDevice<T extends { device: string }>(apply: (items: Iterable<T>) => void, interval: () => number): (item: T) => void {
  const pending = new Map<string, T>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let last = -Infinity;
  const flush = () => {
    clearTimeout(timer);
    timer = undefined;
    if (pageHidden() || !pending.size) return;
    last = performance.now();
    apply(pending.values());
    pending.clear();
  };
  whenVisible(flush);
  return (item) => {
    pending.set(item.device, item);
    if (pageHidden() || timer !== undefined) return;
    timer = setTimeout(() => requestAnimationFrame(flush), Math.max(0, last + interval() - performance.now()));
  };
}
