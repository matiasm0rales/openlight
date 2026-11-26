// Para las pruebas en jsdom: simula que la página se oculta (tray, minimizada) o vuelve a verse.
export function setHidden(hidden: boolean): void {
  Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
  document.dispatchEvent(new Event("visibilitychange"));
}
