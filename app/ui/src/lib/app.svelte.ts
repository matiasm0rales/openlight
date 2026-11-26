// Estado compartido de la app, alimentado por los comandos y los eventos del backend.
import type {
  ConfiguredDevice_Serialize as ConfiguredDevice,
  DeviceHealthEvent,
  FixAction,
  HelpTopic,
  LiveColorEvent,
  OnboardingState,
  PipelineStatsEvent,
  Profile_Serialize as Profile,
  SessionState,
  SettingsSnapshot_Serialize as SettingsSnapshot,
  UserFacingError,
} from "./bindings";
import { EVENT_COLOR_LIVE, EVENT_DEVICE_HEALTH, EVENT_EFFECT_DEVICES, EVENT_ERROR_RAISED, EVENT_PIPELINE_STATS, EVENT_SESSION_STATE } from "./bindings";
import { attempt, commands, on, openExternal, toUserFacing } from "./ipc";
import { go, route } from "./router.svelte";
import { pageHidden, whenVisible } from "./visibility";

export interface RaisedError extends UserFacingError {
  uid: number;
  retry?: () => unknown;
}

export interface StatsSample {
  at: number;
  capture_fps: number;
  sends: Record<string, number>;
}

export const app = $state({
  ready: false,
  onboarding: null as OnboardingState | null,
  devices: [] as ConfiguredDevice[],
  session: "idle" as SessionState,
  health: {} as Record<string, DeviceHealthEvent>,
  live: {} as Record<string, LiveColorEvent>,
  stats: [] as StatsSample[],
  settings: null as SettingsSnapshot | null,
  profiles: [] as Profile[],
  errors: [] as RaisedError[],
  help: null as HelpTopic | null,
  busy: {} as Record<string, boolean>,
});

let uid = 0;

/** Cuántos errores guarda la pila: pasado eso se descarta el más viejo. */
export const MAX_ERRORS = 20;

/** Muestra un error en la pila global (arriba a la derecha). Un error repetido no se apila. */
export function raise(e: unknown, retry?: () => unknown): void {
  const err = toUserFacing(e);
  const same = app.errors.find((x) => x.code === err.code && JSON.stringify(x.params) === JSON.stringify(err.params));
  if (same) return;
  app.errors.push({ ...err, uid: ++uid, retry });
  if (app.errors.length > MAX_ERRORS) app.errors.shift();
}

export function dismiss(errUid: number): void {
  app.errors = app.errors.filter((e) => e.uid !== errUid);
}

export function dismissCode(code: UserFacingError["code"], device?: string): void {
  app.errors = app.errors.filter((e) => !(e.code === code && (!device || e.params.device === device)));
}

/**
 * Ejecuta la acción sugerida de un error. Devuelve `true` solo si la acción lo resolvió seguro
 * (hoy, renovar la clave). Un reintento nunca se da por resuelto: el error se saca de la pila
 * para que, si el reintento vuelve a fallar, aparezca de nuevo con su botón (y no quede
 * descartado por estar repetido).
 */
export async function runFix(action: FixAction, err: UserFacingError, retry?: () => unknown): Promise<boolean> {
  switch (action.kind) {
    case "open_settings": {
      const r = await attempt(() => commands.openSystemSettings(action.value));
      if (!r.ok) raise(r.error);
      return false;
    }
    case "renew_key": {
      const device = err.params.device ?? app.devices[0]?.id;
      if (!device) return false;
      app.busy[`renew:${device}`] = true;
      const r = await attempt(() => commands.renewKey(device));
      app.busy[`renew:${device}`] = false;
      if (!r.ok) {
        raise(r.error);
        return false;
      }
      dismissCode("device_key_rejected", device);
      await refreshDevices();
      return true;
    }
    case "retry": {
      // Sin nada que reintentar (un aviso que mandó el backend), el botón lo descarta: si el
      // problema sigue, el backend lo vuelve a avisar.
      if (!retry) return true;
      if ("uid" in err) dismiss((err as RaisedError).uid);
      try {
        await retry();
      } catch (e) {
        raise(e, retry);
      }
      return false;
    }
    case "remove_from_effect": {
      const device = err.params.device;
      if (!device || !(await setInEffect(device, false))) return false;
      dismissCode("device_in_effect", device);
      return true;
    }
    case "open_url":
      openExternal(action.value);
      return false;
    case "show_help":
      app.help = action.value;
      return false;
  }
}

export async function refreshDevices(): Promise<void> {
  const r = await attempt(() => commands.devicesList());
  if (r.ok) app.devices = r.value;
  else raise(r.error, refreshDevices);
}

export async function refreshSettings(): Promise<void> {
  const [s, p] = await Promise.all([attempt(() => commands.settingsGet()), attempt(() => commands.profilesList())]);
  if (s.ok) app.settings = s.value;
  else raise(s.error, refreshSettings);
  if (p.ok) app.profiles = p.value;
  else raise(p.error, refreshSettings);
}

export async function refreshOnboarding(): Promise<void> {
  const r = await attempt(() => commands.onboardingState());
  if (r.ok) app.onboarding = r.value;
  else raise(r.error, refreshOnboarding);
}

export async function toggleSession(): Promise<void> {
  const running = app.session === "active" || app.session === "connecting" || app.session === "paused";
  app.busy.session = true;
  const r = await attempt(() => (running ? commands.sessionStop() : commands.sessionStart()));
  app.busy.session = false;
  if (r.ok) app.session = r.value;
  else raise(r.error, toggleSession);
}

export async function activateProfile(id: string | null): Promise<void> {
  const r = await attempt(() => commands.profilesActivate(id));
  if (!r.ok) return raise(r.error);
  await refreshSettings();
}

/** "Sigue la pantalla": incluye el foco en el efecto o lo saca. `true` si quedó como se pidió. */
export async function setInEffect(id: string, enabled: boolean): Promise<boolean> {
  app.busy[`effect:${id}`] = true;
  const r = await attempt(() => commands.deviceSetInEffect(id, enabled));
  app.busy[`effect:${id}`] = false;
  if (!r.ok) {
    raise(r.error, () => setInEffect(id, enabled));
    return false;
  }
  markInEffect((d) => (d.id === id ? enabled : d.in_effect));
  return true;
}

function markInEffect(value: (d: ConfiguredDevice) => boolean): void {
  for (const d of app.devices) d.in_effect = value(d);
}

export function deviceName(id: string): string {
  return app.devices.find((d) => d.id === id)?.name ?? id;
}

/**
 * El estado en vivo de cada foco. Con la página oculta solo se guarda lo último (sin tocar el
 * estado, así no se redibuja a escondidas) y se aplica al volver a verla. El backend ya no manda
 * estos eventos con las ventanas ocultas; esto cubre lo que él no detecta (una ventana tapada).
 */
function liveStore<T extends { device: string }>(target: () => Record<string, T>): (e: T) => void {
  const later = new Map<string, T>();
  whenVisible(() => {
    for (const [device, e] of later) target()[device] = e;
    later.clear();
  });
  return (e) => {
    if (pageHidden()) {
      later.set(e.device, e);
      return;
    }
    later.delete(e.device);
    target()[e.device] = e;
  };
}

/** Las estadísticas llegan una vez por segundo: más que esto sin ninguna es un hueco. */
const STATS_GAP_MS = 3000;

/**
 * El historial, si sigue al día. El gráfico dibuja las muestras una al lado de la otra: tras un
 * hueco (ventana oculta, efecto detenido) las viejas parecerían de recién y se descartan.
 */
function recentStats(): StatsSample[] {
  const last = app.stats.at(-1);
  return last && Date.now() - last.at <= STATS_GAP_MS ? app.stats : [];
}

let started = false;

export async function init(): Promise<void> {
  if (started) return;
  started = true;
  whenVisible(() => {
    app.stats = recentStats();
  });
  await Promise.all([
    on(EVENT_SESSION_STATE, (e) => {
      app.session = e.state;
    }),
    on(EVENT_DEVICE_HEALTH, liveStore(() => app.health)),
    on(EVENT_COLOR_LIVE, liveStore(() => app.live)),
    on(EVENT_ERROR_RAISED, (e) => raise(e)),
    // También cuando se cambia desde el tray.
    on(EVENT_EFFECT_DEVICES, (e) => markInEffect((d) => !e.excluded.includes(d.id))),
    on(EVENT_PIPELINE_STATS, (e: PipelineStatsEvent) => {
      if (pageHidden()) return;
      const sample: StatsSample = {
        at: Date.now(),
        capture_fps: e.capture_fps ?? 0,
        sends: Object.fromEntries(e.devices.map((d) => [d.device, d.sends_per_s ?? 0])),
      };
      app.stats = [...recentStats().slice(-59), sample];
    }),
  ]);
  // La ventana principal se destruye al cerrarla, así que cada vez que se abre esta página arranca
  // de cero: ya con los oyentes puestos, el backend le manda lo último de cada foco y los errores
  // que salieron mientras no estaba. La de calibración no lo pide: se los llevaría ella.
  if (route.name !== "calibrate") {
    const ready = await attempt(() => commands.uiReady());
    if (!ready.ok) raise(ready.error);
  }
  const session = await attempt(() => commands.sessionState());
  if (session.ok) app.session = session.value;
  await Promise.all([refreshOnboarding(), refreshDevices(), refreshSettings()]);
  app.ready = true;
  // Las rutas que abre la app nativa en su propia ventana (calibración a pantalla completa) no se
  // desvían al asistente aunque falte terminarlo: la calibración es justo el paso 10.
  const propia = ["#/tray", "#/calibrate"].some((r) => location.hash.startsWith(r));
  if (app.onboarding && app.onboarding.step !== "done" && !propia) go("onboarding");
}
