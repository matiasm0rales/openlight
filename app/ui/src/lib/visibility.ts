// La ventana puede estar fuera de la vista (en el tray, minimizada o tapada). Ahí no tiene sentido
// dibujar ni pedirle nada al backend.

export function pageHidden(): boolean {
  return typeof document !== "undefined" && document.hidden;
}

/** Llama a `run` cada vez que la página vuelve a verse. Devuelve cómo dejar de avisar. */
export function whenVisible(run: () => unknown): () => void {
  if (typeof document === "undefined") return () => {};
  const change = () => {
    if (!document.hidden) void run();
  };
  document.addEventListener("visibilitychange", change);
  return () => document.removeEventListener("visibilitychange", change);
}

/**
 * Como `setInterval`, pero solo con la página a la vista: oculta no corre, y al volver corre
 * enseguida. Devuelve cómo pararlo.
 */
export function everyWhileVisible(run: () => unknown, ms: number): () => void {
  let timer: ReturnType<typeof setInterval> | undefined;
  const start = () => {
    timer ??= setInterval(() => void run(), ms);
  };
  const pause = () => {
    clearInterval(timer);
    timer = undefined;
  };
  const change = () => {
    if (pageHidden()) return pause();
    void run();
    start();
  };
  if (!pageHidden()) start();
  if (typeof document !== "undefined") document.addEventListener("visibilitychange", change);
  return () => {
    pause();
    if (typeof document !== "undefined") document.removeEventListener("visibilitychange", change);
  };
}
