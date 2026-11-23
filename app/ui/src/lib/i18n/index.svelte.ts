// i18n mínimo: los textos viven en es.json / en.json; las claves las mandan también el backend
// (títulos de ajustes, errores) y la UI las resuelve acá.
import es from "./es.json";
import en from "./en.json";

export type Locale = "es" | "en";
type Tree = { [k: string]: string | Tree };
const LOCALES: Record<Locale, Tree> = { es: es as Tree, en: en as Tree };
const STORAGE_KEY = "openlight.locale";

function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "es" || saved === "en") return saved;
  } catch {
    // sin localStorage (pruebas, modo privado): se usa el idioma del sistema
  }
  const nav = typeof navigator !== "undefined" ? navigator.language : "es";
  return nav.toLowerCase().startsWith("es") ? "es" : "en";
}

export const i18n = $state<{ locale: Locale }>({ locale: detect() });

export function setLocale(locale: Locale): void {
  i18n.locale = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // preferencia de comodidad: si no se puede guardar, no pasa nada
  }
  if (typeof document !== "undefined") document.documentElement.lang = locale;
}

function lookup(tree: Tree, key: string): string | undefined {
  let node: string | Tree | undefined = tree;
  for (const part of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = node[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** ¿Existe la clave en el idioma actual (o en español)? */
export function has(key: string): boolean {
  return lookup(LOCALES[i18n.locale], key) !== undefined || lookup(LOCALES.es, key) !== undefined;
}

/**
 * Resuelve una clave e interpola `{param}`. Si un parámetro tiene su propia clave
 * (`region.us`, `settings.<id>.title`), se muestra traducido.
 */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const raw = lookup(LOCALES[i18n.locale], key) ?? lookup(LOCALES.es, key) ?? key;
  return raw.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    if (value === undefined) return whole;
    const s = String(value);
    for (const candidate of [`${name}.${s}`, `settings.${s}.title`]) {
      const translated = lookup(LOCALES[i18n.locale], candidate);
      if (translated) return translated;
    }
    return s;
  });
}

export function locales(): Locale[] {
  return Object.keys(LOCALES) as Locale[];
}
