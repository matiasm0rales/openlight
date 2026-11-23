// Tema claro/oscuro: por defecto sigue al sistema. La elección es una comodidad local.
export type Theme = "system" | "light" | "dark";
const KEY = "openlight.theme";

function load(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // sin localStorage: se usa el del sistema
  }
  return "system";
}

export const theme = $state<{ value: Theme }>({ value: load() });

export function applyTheme(value: Theme): void {
  theme.value = value;
  try {
    localStorage.setItem(KEY, value);
  } catch {
    // no pasa nada si no se puede recordar
  }
  if (value === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = value;
}
