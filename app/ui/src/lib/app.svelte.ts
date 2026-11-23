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
import { EVENT_COLOR_LIVE, EVENT_DEVICE_HEALTH, EVENT_ERROR_RAISED, EVENT_PIPELINE_STATS, EVENT_SESSION_STATE } from "./bindings";
import { attempt, commands, on, openExternal, toUserFacing } from "./ipc";
import { go } from "./router.svelte";

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

/** Muestra un error en la pila global (arriba a la derecha). Un error repetido no se apila. */
export function raise(e: unknown, retry?: () => unknown): void {
  const err = toUserFacing(e);
  const same = app.errors.find((x) => x.code === err.code && JSON.stringify(x.params) === JSON.stringify(err.params));
  if (same) return;
  app.errors.push({ ...err, uid: ++uid, retry });
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

export function deviceName(id: string): string {
  return app.devices.find((d) => d.id === id)?.name ?? id;
}

let started = false;

export async function init(): Promise<void> {
  if (started) return;
  started = true;
  await Promise.all([
    on(EVENT_SESSION_STATE, (e) => {
      app.session = e.state;
    }),
    on(EVENT_DEVICE_HEALTH, (e) => {
      app.health[e.device] = e;
    }),
    on(EVENT_COLOR_LIVE, (e) => {
      app.live[e.device] = e;
    }),
    on(EVENT_ERROR_RAISED, (e) => raise(e)),
    on(EVENT_PIPELINE_STATS, (e: PipelineStatsEvent) => {
      const sample: StatsSample = {
        at: Date.now(),
        capture_fps: e.capture_fps ?? 0,
        sends: Object.fromEntries(e.devices.map((d) => [d.device, d.sends_per_s ?? 0])),
      };
      app.stats = [...app.stats.slice(-59), sample];
    }),
  ]);
  const session = await attempt(() => commands.sessionState());
  if (session.ok) app.session = session.value;
  await Promise.all([refreshOnboarding(), refreshDevices(), refreshSettings()]);
  app.ready = true;
  if (app.onboarding && app.onboarding.step !== "done" && !location.hash.startsWith("#/tray")) go("onboarding");
}
