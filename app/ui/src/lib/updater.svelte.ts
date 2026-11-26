// Actualizaciones automáticas (tauri-plugin-updater): busca al arrancar y cada 6 horas (si la
// búsqueda falla, reintenta con esperas cada vez más largas), y cuando la persona lo pide
// descarga, devuelve los focos a como estaban, instala y reinicia.
//
// Nada se instala sin que se toque "Actualizar y reiniciar".
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { raise } from "./app.svelte";
import { attempt, commands } from "./ipc";
import {
  CHECK_EVERY_MS,
  CHECK_TIMEOUT_MS,
  DOWNLOAD_TIMEOUT_MS,
  REOPEN_MIN_MS,
  TICK_MS,
  classifyUpdateError,
  noRelease,
  plainNotes,
  retryDelay,
  type UpdateErrorKind,
  type UpdateStep,
} from "./updates";

/** `restoring`: devolviendo los focos antes de instalar. */
export type UpdatePhase = "idle" | "checking" | "downloading" | "restoring" | "installing" | "restarting";

export interface UpdateProblem {
  step: UpdateStep;
  kind: UpdateErrorKind;
  /** El mensaje original, para el diagnóstico. */
  cause: string;
}

// Preferencia local, como el tema y el idioma.
const AUTO_KEY = "openlight.updates.auto";

function savedAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "off";
  } catch {
    return true; // sin localStorage: prendida, que es el valor por defecto
  }
}

export const updates = $state({
  auto: savedAuto(),
  /** La versión instalada (`null` hasta que se lee). */
  version: null as string | null,
  phase: "idle" as UpdatePhase,
  /** La última búsqueda no encontró nada más nuevo. */
  upToDate: false,
  available: null as { version: string; current: string; notes: string } | null,
  progress: { done: 0, total: null as number | null },
  error: null as UpdateProblem | null,
  /** La versión nueva ya quedó instalada: solo falta reiniciar. */
  installed: false,
  /** Versión que se pospuso con "Más tarde". */
  dismissed: null as string | null,
});

/** El recurso del plugin detrás de `updates.available`. */
let pending: Update | null = null;
/** Cuándo empezó la última búsqueda, haya salido bien o no (0 = ninguna). */
let lastAttempt = 0;
/** Desde cuándo toca la próxima búsqueda automática. */
let nextCheck = 0;
/** Búsquedas fallidas seguidas: cada una alarga la espera. */
let failures = 0;
let stop: (() => void) | null = null;

/** Hay una instalación en curso. */
export function updating(): boolean {
  return updates.phase !== "idle" && updates.phase !== "checking";
}

/** Si el panel tiene que mostrar el aviso. */
export function noticeVisible(): boolean {
  return updates.available !== null && (updating() || updates.dismissed !== updates.available.version);
}

export function setAutoCheck(auto: boolean): void {
  updates.auto = auto;
  try {
    localStorage.setItem(AUTO_KEY, auto ? "on" : "off");
  } catch {
    // no pasa nada si no se puede recordar
  }
  if (auto) tick();
}

export function dismissUpdate(): void {
  updates.dismissed = updates.available?.version ?? null;
  updates.error = null;
}

/**
 * Busca una versión nueva. La búsqueda automática falla en silencio (sin red no hay nada que
 * molestar); la que pide la persona muestra el error.
 */
export async function checkForUpdates(manual = false): Promise<void> {
  if (updates.phase !== "idle" || updates.installed) return;
  updates.phase = "checking";
  lastAttempt = Date.now();
  if (manual) {
    updates.upToDate = false;
    if (updates.error?.step === "check") updates.error = null;
  }
  try {
    const found = await check({ timeout: CHECK_TIMEOUT_MS }).catch((cause: unknown) => {
      // Sin ninguna versión publicada se está al día. Con una ya ofrecida, en cambio, es el
      // servidor que falla: la oferta no se descarta.
      if (noRelease(cause) && !pending) return null;
      throw cause;
    });
    failures = 0;
    nextCheck = lastAttempt + CHECK_EVERY_MS;
    // Un error de descarga o instalación se conserva mientras la versión ofrecida sea la misma.
    if (updates.error && (updates.error.step === "check" || found?.version !== updates.available?.version)) updates.error = null;
    void pending?.close().catch(() => {});
    pending = found;
    updates.available = found ? { version: found.version, current: found.currentVersion, notes: plainNotes(found.body) } : null;
    updates.upToDate = !found;
    if (manual) updates.dismissed = null;
  } catch (cause) {
    nextCheck = lastAttempt + retryDelay(++failures);
    if (manual) updates.error = problem("check", cause);
    else console.warn("no se pudo buscar actualizaciones:", cause);
  } finally {
    updates.phase = "idle";
  }
}

function problem(step: UpdateStep, cause: unknown, kind = classifyUpdateError(cause, step)): UpdateProblem {
  const text = typeof cause === "object" && cause !== null && "code" in cause ? String(cause.code) : String(cause);
  return { step, kind, cause: text };
}

function onProgress(event: DownloadEvent): void {
  if (event.event === "Started") updates.progress = { done: 0, total: event.data.contentLength ?? null };
  else if (event.event === "Progress") updates.progress.done += event.data.chunkLength;
}

/**
 * "Actualizar y reiniciar". Primero descarga (si falla no se tocó nada); después devuelve los
 * focos como al salir, porque en Windows instalar cierra la app en el acto.
 */
export async function installUpdate(): Promise<void> {
  const update = pending;
  if (!update || updates.phase !== "idle" || updates.installed) return;
  updates.error = null;
  updates.progress = { done: 0, total: null };
  updates.phase = "downloading";
  // La instalación la lleva adelante esta página: mientras dure, cerrar la ventana la esconde en
  // vez de destruirla.
  await holdWindow(true);
  try {
    await update.download(onProgress, { timeout: DOWNLOAD_TIMEOUT_MS });
  } catch (cause) {
    updates.error = problem("download", cause);
    updates.phase = "idle";
    await holdWindow(false);
    return;
  }

  updates.phase = "restoring";
  const session = await attempt(() => commands.sessionState());
  const wasRunning = session.ok && ["connecting", "active", "paused"].includes(session.value);
  try {
    await commands.updatePrepare();
    updates.phase = "installing";
    await update.install();
  } catch (cause) {
    // La versión actual sigue andando: se deshace la preparación.
    await attempt(() => commands.updateAbort());
    if (wasRunning) {
      const restarted = await attempt(() => commands.sessionStart());
      if (!restarted.ok) raise(restarted.error);
    }
    updates.error = problem("install", cause);
    updates.phase = "idle";
    await holdWindow(false);
    return;
  }

  updates.installed = true;
  await relaunchNow();
}

/** Reintento cuando la versión nueva quedó instalada pero la app no se pudo reiniciar. */
export async function restartApp(): Promise<void> {
  if (updates.phase !== "idle" || !updates.installed) return;
  updates.error = null;
  updates.phase = "restoring";
  await attempt(() => commands.updatePrepare());
  await relaunchNow();
}

/** Si falla no se frena la actualización: a lo sumo, cerrar la ventana la interrumpe. */
async function holdWindow(hold: boolean): Promise<void> {
  const r = await attempt(() => commands.uiHold(hold));
  if (!r.ok) console.warn("no se pudo retener la ventana:", r.error);
}

async function relaunchNow(): Promise<void> {
  updates.phase = "restarting";
  try {
    await relaunch();
  } catch (cause) {
    await attempt(() => commands.updateAbort());
    updates.error = problem("install", cause, "restart");
    updates.phase = "idle";
  }
}

/** Busca si ya toca. `checkForUpdates` no encima dos búsquedas. */
function tick(): void {
  if (stop && updates.auto && Date.now() >= nextCheck) void checkForUpdates();
}

/** Arranca la búsqueda automática (solo en la ventana principal). Devuelve cómo detenerla. */
export function startUpdater(): () => void {
  if (stop) return stop;
  void getVersion()
    .then((v) => (updates.version = v))
    .catch((cause) => console.warn("no se pudo leer la versión:", cause));
  const timer = setInterval(tick, TICK_MS);
  // Los temporizadores no corren con la compu suspendida ni son puntuales con la ventana oculta:
  // también se mira al volver a mostrarla. macOS puede avisar eso muchas veces por segundo (cada
  // vez que otra ventana la tapa y la destapa): nunca busca antes de `REOPEN_MIN_MS`.
  const onVisible = () => {
    if (document.visibilityState === "visible" && Date.now() - lastAttempt >= REOPEN_MIN_MS) tick();
  };
  document.addEventListener("visibilitychange", onVisible);
  stop = () => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", onVisible);
    stop = null;
  };
  tick();
  return stop;
}
