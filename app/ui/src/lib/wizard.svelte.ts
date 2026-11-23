// Estado del asistente que vive mientras está abierto (lo persistente lo guarda el backend con
// `onboarding_save`; las credenciales, en el llavero).
import type { CloudDeviceInfo_Serialize, Region } from "./bindings";

export const ONBOARDING_STEPS = [
  "welcome",
  "permissions",
  "tuya_account",
  "cloud_project",
  "link_app",
  "credentials",
  "choose_devices",
  "test_devices",
  "zones",
  "calibrate",
  "done",
] as const;

export const wizard = $state({
  confirmed: {} as Record<string, boolean>,
  region: null as Region | null,
  cloud: [] as CloudDeviceInfo_Serialize[],
  selected: [] as string[],
  tested: {} as Record<string, "yes" | "no">,
  /** Adónde vuelve la calibración cuando termina. */
  returnTo: null as string | null,
});
