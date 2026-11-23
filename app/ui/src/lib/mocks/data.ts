// Datos falsos pero realistas: los dos focos del plan (W509Z2 en 3.5 y A60 BAW en 3.3), un foco
// solo blanco, un enchufe y una tira desconectada. Ids, IPs y uids son inventados.
import type {
  Capabilities_Serialize,
  CloudDeviceInfo_Serialize,
  ConfiguredDevice_Serialize,
  DisplayInfo,
  LogLine,
  Profile_Serialize,
  Zone,
  ZonePreset,
} from "../bindings";
import { presetZone } from "../zones";

export const IDS = {
  living: "bf3a9c1e7d2b4f6a8c0e1d",
  desk: "eb81f0c2a94d37e6b5a1c8",
  hall: "bf77d2e019c4a8b3f6e5a0",
  plug: "eb0d4c8a1f2e3b5a6c7d9e",
  strip: "bf5e6a7c8d9e0f1a2b3c4d",
} as const;

const CAPS_W509: Capabilities_Serialize = {
  dp_switch: "20",
  dp_mode: "21",
  dp_bright: { dp: "22", min: 10, max: 1000, step: 1 },
  dp_temp: { dp: "23", min: 0, max: 1000, step: 1 },
  dp_colour: ["24", "v2_hsv12"],
  dp_control: "28",
  modes: ["white", "colour", "scene", "music"],
};

const CAPS_A60: Capabilities_Serialize = {
  dp_switch: "20",
  dp_mode: "21",
  dp_bright: { dp: "22", min: 10, max: 1000, step: 1 },
  dp_temp: { dp: "23", min: 0, max: 1000, step: 1 },
  dp_colour: ["24", "v2_hsv12"],
  dp_control: null,
  modes: ["white", "colour", "scene"],
};

const CAPS_WHITE: Capabilities_Serialize = {
  dp_switch: "1",
  dp_mode: "2",
  dp_bright: { dp: "3", min: 25, max: 255, step: 1 },
  dp_temp: { dp: "4", min: 0, max: 255, step: 1 },
  dp_colour: null,
  dp_control: null,
  modes: ["white"],
};

export const CLOUD_DEVICES: CloudDeviceInfo_Serialize[] = [
  {
    id: IDS.living,
    name: "Foco living",
    category: "dj",
    product_id: "keyw509z2rgbcw",
    is_light: true,
    online: true,
    capabilities: CAPS_W509,
    lan: { id: IDS.living, ip: "192.168.1.42", version: "3.5", product_key: "keyw509z2rgbcw" },
  },
  {
    id: IDS.desk,
    name: "Lámpara escritorio",
    category: "dj",
    product_id: "keya60bawrgbcw",
    is_light: true,
    online: true,
    capabilities: CAPS_A60,
    lan: { id: IDS.desk, ip: "192.168.1.57", version: "3.3", product_key: "keya60bawrgbcw" },
  },
  {
    id: IDS.hall,
    name: "Foco pasillo",
    category: "dj",
    product_id: "keywhite9w2700",
    is_light: true,
    online: true,
    capabilities: CAPS_WHITE,
    lan: { id: IDS.hall, ip: "192.168.1.63", version: "3.3", product_key: "keywhite9w2700" },
  },
  {
    id: IDS.strip,
    name: "Tira LED tele",
    category: "dd",
    product_id: "keystrip5mrgb",
    is_light: true,
    online: false,
    capabilities: { ...CAPS_A60, modes: ["white", "colour"] },
    lan: null,
  },
  {
    id: IDS.plug,
    name: "Enchufe cafetera",
    category: "cz",
    product_id: "keyplug16aeu",
    is_light: false,
    online: true,
    capabilities: null,
    lan: { id: IDS.plug, ip: "192.168.1.71", version: "3.3", product_key: "keyplug16aeu" },
  },
];

export const DISPLAYS: DisplayInfo[] = [
  { id: "69733382", name: "LG UltraFine 27", width: 3840, height: 2160, builtin: false, primary: true },
  { id: "1", name: "Pantalla integrada", width: 3024, height: 1964, builtin: true, primary: false },
];

export function zone(preset: Exclude<ZonePreset, "custom">, display = DISPLAYS[0].id): Zone {
  return presetZone(preset, display);
}

export function configuredFrom(cloud: CloudDeviceInfo_Serialize, ip: string | null): ConfiguredDevice_Serialize {
  return {
    id: cloud.id,
    name: cloud.name,
    ip: ip ?? cloud.lan?.ip ?? null,
    auto_ip: ip === null,
    version: cloud.lan?.version ?? "3.3",
    capabilities: cloud.capabilities ?? CAPS_WHITE,
    zones: [zone("full")],
    overrides: {},
    calibrated: false,
  };
}

export function builtinProfiles(): Profile_Serialize[] {
  return [
    {
      id: "movies",
      name: "profile.movies",
      values: { smoothing_ms: 500, saturation: 1.4, brightness_max: 0.8 },
      shortcut: "CmdOrCtrl+Alt+1",
      builtin: true,
    },
    {
      id: "games",
      name: "profile.games",
      values: { smoothing_ms: 120, capture_fps: 30, max_sends_per_s: 10 },
      shortcut: "CmdOrCtrl+Alt+2",
      builtin: true,
    },
    {
      id: "work",
      name: "profile.work",
      values: { saturation: 1.0, brightness_max: 0.5, smoothing_ms: 800 },
      shortcut: null,
      builtin: true,
    },
  ];
}

/** Log de arranque, con los cortes que motivaron la investigación del plan (EOF del foco). */
export function seedLog(now: number): LogLine[] {
  const ago = (s: number) => now - s * 1000;
  return [
    { at_ms: ago(3600), level: "info", target: "openlight_core", message: "OpenLight 0.1.0 arrancó (macOS 26.5, arm64)" },
    { at_ms: ago(3599), level: "info", target: "capture", message: "ScreenCaptureKit: LG UltraFine 27 a 96×54, 20 fps" },
    { at_ms: ago(3598), level: "info", target: "tuya_device", message: "Foco living: sesión 3.5 negociada con 192.168.1.42 (rtt 21 ms)" },
    { at_ms: ago(2710), level: "warn", target: "tuya_device", message: "Foco living: el foco cerró el socket (EOF); reconecto en 3 s" },
    { at_ms: ago(2707), level: "info", target: "tuya_device", message: "Foco living: reconectado; vuelvo a poner switch=true, mode=colour" },
    { at_ms: ago(1220), level: "warn", target: "tuya_device", message: "Foco living: sin respuesta durante 30 s; reconecto" },
    { at_ms: ago(1216), level: "info", target: "tuya_device", message: "Foco living: reconectado" },
    { at_ms: ago(640), level: "error", target: "tuya_device", message: "Lámpara escritorio: la clave fue rechazada (914)" },
    { at_ms: ago(30), level: "debug", target: "openlight_core", message: "Perfil activo: ninguno; 2 focos, 2 zonas" },
  ];
}
