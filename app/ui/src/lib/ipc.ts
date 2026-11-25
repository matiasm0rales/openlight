// Única puerta hacia el backend: los comandos, tipos y nombres de eventos salen de `bindings.ts`
// (generado por tauri-specta). Acá solo hay ayudas para manejar errores y escuchar eventos.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  EVENT_COLOR_LIVE,
  EVENT_DEVICE_HEALTH,
  EVENT_DISCOVERY_FOUND,
  EVENT_EFFECT_DEVICES,
  EVENT_ERROR_RAISED,
  EVENT_PIPELINE_STATS,
  EVENT_SESSION_STATE,
  type DeviceHealthEvent,
  type DiscoveredDevice,
  type EffectDevicesEvent,
  type LiveColorEvent,
  type PipelineStatsEvent,
  type SessionStateEvent,
  type UserFacingError,
} from "./bindings";

export { commands };

/** Payload de cada evento del backend. */
export interface EventPayloads {
  [EVENT_SESSION_STATE]: SessionStateEvent;
  [EVENT_DEVICE_HEALTH]: DeviceHealthEvent;
  [EVENT_COLOR_LIVE]: LiveColorEvent;
  [EVENT_ERROR_RAISED]: UserFacingError;
  [EVENT_DISCOVERY_FOUND]: DiscoveredDevice;
  [EVENT_PIPELINE_STATS]: PipelineStatsEvent;
  [EVENT_EFFECT_DEVICES]: EffectDevicesEvent;
}

export type EventName = keyof EventPayloads;

export function on<E extends EventName>(
  event: E,
  handler: (payload: EventPayloads[E]) => void,
): Promise<UnlistenFn> {
  return listen<EventPayloads[E]>(event, (e) => handler(e.payload));
}

export function isUserFacingError(e: unknown): e is UserFacingError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as UserFacingError).code === "string" &&
    typeof (e as UserFacingError).title === "string" &&
    Array.isArray((e as UserFacingError).actions)
  );
}

/** Convierte cualquier cosa que tire un comando en un `UserFacingError` mostrable. */
export function toUserFacing(e: unknown): UserFacingError {
  if (isUserFacingError(e)) return { ...e, params: e.params ?? {} };
  return {
    code: "internal",
    title: "error.internal.title",
    detail: "error.internal.detail",
    params: { cause: String(e) },
    actions: [{ kind: "retry" }, { kind: "show_help", value: "device_troubleshooting" }],
  };
}

export type Result<T> = { ok: true; value: T } | { ok: false; error: UserFacingError };

/** Corre un comando y devuelve el error en vez de tirarlo. */
export async function attempt<T>(run: () => Promise<T>): Promise<Result<T>> {
  try {
    return { ok: true, value: await run() };
  } catch (e) {
    return { ok: false, error: toUserFacing(e) };
  }
}

/** Abre una URL en el navegador del sistema (plugin opener; I1 lo registra en src-tauri). */
export function openExternal(url: string): void {
  openUrl(url).catch(() => window.open(url, "_blank", "noopener,noreferrer"));
}
