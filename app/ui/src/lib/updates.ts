// Reglas de las actualizaciones que no dependen de la pantalla: plazos, qué clase de error es cada
// mensaje del updater, qué acciones se sugieren y cómo se muestran las notas de la versión.

/** Cada cuánto se busca una versión nueva con la búsqueda automática prendida. */
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
/** Cada cuánto se mira si ya toca buscar (y se reintenta una búsqueda que falló). */
export const TICK_MS = 15 * 60 * 1000;
export const CHECK_TIMEOUT_MS = 30 * 1000;
export const DOWNLOAD_TIMEOUT_MS = 15 * 60 * 1000;
/** De dónde bajar la versión nueva a mano. */
export const RELEASES_URL = "https://github.com/matiasm0rales/openlight/releases/latest";

export type UpdateStep = "check" | "download" | "install";
export type UpdateErrorKind = "network" | "unavailable" | "signature" | "unsupported" | "permission" | "auth" | "install" | "restart";
export type UpdateFix = "retry" | "web" | "restart";

// El orden importa: gana la primera que coincide.
const PATTERNS: [UpdateErrorKind, RegExp, UpdateStep[]?][] = [
  ["unsupported", /the platform .* was not found|fallback platforms|unsupported (os|application architecture)/i],
  ["auth", /authentication failed/i],
  // minisign, base64 (la firma o la clave pública mal copiadas) y la versión firmada.
  ["signature", /signature|minisign|signed for version|invalid (symbol|byte|padding|last symbol|input length)/i],
  // Windows traduce el texto del sistema: queda el número.
  ["permission", /permission denied|read-only file system|operation not permitted|access is denied|os error (5|13|30)\b|same mount point/i, ["install"]],
  ["network", /error decoding response body/i, ["download"]],
  ["unavailable", /valid release json|download request failed|error decoding response|missing field|expected value|invalid type/i],
  ["network", /error sending request|network|dns|connect|timed out|timeout|unreachable|offline/i],
];

const FALLBACK: Record<UpdateStep, UpdateErrorKind> = { check: "unavailable", download: "network", install: "install" };

export function classifyUpdateError(cause: unknown, step: UpdateStep): UpdateErrorKind {
  // Sin las URL: las de descarga de GitHub tienen palabras como "signature" en la consulta.
  const message = String(cause instanceof Error ? cause.message : cause).replace(/https?:\/\/[^\s)]+/g, "");
  const found = PATTERNS.find(([, pattern, steps]) => (!steps || steps.includes(step)) && pattern.test(message));
  return found?.[0] ?? FALLBACK[step];
}

const FIXES: Record<UpdateErrorKind, UpdateFix[]> = {
  network: ["retry"],
  unavailable: ["retry", "web"],
  signature: ["retry", "web"],
  unsupported: ["web"],
  permission: ["retry", "web"],
  auth: ["retry"],
  install: ["retry", "web"],
  restart: ["restart"],
};

/** Las acciones que se sugieren; la primera es la principal. */
export function updateFixes(kind: UpdateErrorKind): UpdateFix[] {
  return FIXES[kind];
}

/**
 * Las notas llegan en el markdown de la Release de GitHub y se muestran como texto (nunca como
 * HTML): se quitan las marcas que molestan al leer.
 */
export function plainNotes(markdown: string | null | undefined): string {
  return (markdown ?? "")
    .replace(/\r/g, "")
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/^[ \t]*[-*+][ \t]+/gm, "• ")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\*\*([^*\n]+)\*\*/g, "$1")
    .replace(/(^|\W)__([^_\n]+)__(?=\W|$)/gm, "$1$2")
    .replace(/`([^`\n]+)`/g, "$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
