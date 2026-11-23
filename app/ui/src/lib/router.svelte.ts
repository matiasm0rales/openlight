// Router por hash: la app es una sola ventana y el hash sobrevive a recargar en el navegador.
export type RouteName = "onboarding" | "dashboard" | "settings" | "devices" | "diagnostics" | "manual" | "calibrate" | "tray";

const NAMES: RouteName[] = ["onboarding", "dashboard", "settings", "devices", "diagnostics", "manual", "calibrate", "tray"];

export interface Route {
  name: RouteName;
  param: string | null;
}

export function parse(hash: string): Route {
  const [name, param] = hash.replace(/^#\/?/, "").split("/");
  const found = NAMES.find((n) => n === name);
  return { name: found ?? "dashboard", param: param ? decodeURIComponent(param) : null };
}

export const route = $state<Route>(parse(typeof location !== "undefined" ? location.hash : ""));

export function go(name: RouteName, param: string | null = null): void {
  const hash = `#/${name}${param ? `/${encodeURIComponent(param)}` : ""}`;
  if (location.hash !== hash) location.hash = hash;
  else Object.assign(route, parse(hash));
}

if (typeof window !== "undefined") {
  window.addEventListener("hashchange", () => Object.assign(route, parse(location.hash)));
}
