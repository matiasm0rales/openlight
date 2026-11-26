// Backend falso para correr la UI en el navegador sin Tauri (`pnpm --filter openlight-ui dev`)
// y para Playwright. Responde los mismos comandos que `app/src-tauri/src/ipc.rs` y emite los mismos
// eventos, con fallas realistas (914, región equivocada, Access Secret mal copiado...).
//
// Escenarios por URL: `?scenario=fresh` (asistente desde cero), `?scenario=configured` (2 focos,
// uno con la clave rechazada) o `?scenario=four` (4 focos, para las distribuciones y el orden).
// `?fast=1` acorta las demoras (lo usan las pruebas).
//
// Pistas para probar el paso 6 (credenciales):
// - Access Secret de menos o más de 32 caracteres → firma inválida.
// - Access ID que empieza con "expired" → venció la prueba de IoT Core.
// - Access ID que empieza con "nolink" → Smart Life no vinculada.
// - Cualquier Data Center que no sea "Western America" → región equivocada (sugiere `us`).
import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import {
  EVENT_COLOR_LIVE,
  EVENT_DEVICE_HEALTH,
  EVENT_DISCOVERY_FOUND,
  EVENT_EFFECT_DEVICES,
  EVENT_ERROR_RAISED,
  EVENT_PIPELINE_STATS,
  EVENT_SESSION_STATE,
  type BackupSummary,
  type CalibrationPoint,
  type CalibrationState,
  type CloudCredentialsInput,
  type ConfiguredDevice_Serialize,
  type DeviceAddRequest_Deserialize,
  type DeviceHealthEvent_Serialize,
  type DeviceSessionState,
  type DisconnectReason,
  type DuplicatePolicy,
  type ErrorCode,
  type FixAction,
  type Hsv,
  type ImportReport,
  type LinkHealth_Serialize,
  type LogLine,
  type ManualSet,
  type MissingKeysReport,
  type OnboardingState,
  type OnboardingStep,
  type PermissionKind,
  type PermissionState,
  type Profile_Deserialize,
  type Profile_Serialize,
  type Region,
  type SessionState,
  type SettingId,
  type SettingScope,
  type SettingValue,
  type Settings,
  type SettingsOverride,
  type SettingsSnapshot_Serialize,
  type SystemPane,
  type UserFacingError,
  type Zone,
} from "../bindings";
import { CATALOG, defaultSettings } from "./catalog";
import { CLOUD_DEVICES, DISPLAYS, IDS, builtinProfiles, configuredFrom, seedLog, zone } from "./data";
import { MOCK_VERSION, MockUpdater } from "./updater";

export type Scenario = "fresh" | "configured" | "four";

interface MockState {
  onboarding: { step: OnboardingStep; completed: OnboardingStep[] };
  credentials: { access_id: string; region: Region } | null;
  devices: ConfiguredDevice_Serialize[];
  global: Settings;
  deviceOverrides: Record<string, SettingsOverride>;
  profiles: Profile_Serialize[];
  activeProfile: string | null;
  permissions: Record<PermissionKind, PermissionState>;
  keyRejected: string[];
  calibrations: Record<string, CalibrationPoint[]>;
  /** Focos sin clave (los que llegaron de un respaldo). */
  noKey?: string[];
}

const STORAGE_KEY = "openlight.mock";
const CORRECT_REGION: Region = "us";
const HUES = [0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330];
const QUICK_HUES = [0, 60, 120, 180, 240, 300];

function freshState(): MockState {
  return {
    onboarding: { step: "welcome", completed: [] },
    credentials: null,
    devices: [],
    global: defaultSettings(),
    deviceOverrides: {},
    profiles: builtinProfiles(),
    activeProfile: null,
    permissions: { screen_recording: "not_determined", local_network: "granted", firewall: "not_applicable", screen_cast_portal: "not_applicable" },
    keyRejected: [],
    calibrations: {},
  };
}

function configuredState(): MockState {
  const s = freshState();
  const all = Object.values(ALL_STEPS);
  s.onboarding = { step: "done", completed: all };
  s.credentials = { access_id: "p9xk2wq7vd4mfa1ztc8e", region: CORRECT_REGION };
  const living = configuredFrom(CLOUD_DEVICES[0], null);
  living.zones = [zone("left")];
  living.calibrated = true;
  const desk = configuredFrom(CLOUD_DEVICES[1], "192.168.1.57");
  desk.zones = [zone("right")];
  desk.overrides = { brightness_max: 0.7 };
  s.devices = [living, desk];
  s.deviceOverrides = { [desk.id]: { brightness_max: 0.7 } };
  s.permissions.screen_recording = "granted";
  s.keyRejected = [desk.id];
  s.calibrations[living.id] = HUES.map((h) => ({ screen_hue_deg: h, tono_deg: h + (h >= 180 && h <= 270 ? -12 : 4), sat: h === 60 ? 1.2 : 1 }));
  return s;
}

/** Los dos de siempre y dos más: lo que hace falta para repartir las cuatro esquinas. */
function fourState(): MockState {
  const s = configuredState();
  const more = [
    { id: IDS.shelf, name: "Foco repisa", ip: "192.168.1.81" },
    { id: IDS.ceiling, name: "Foco techo", ip: "192.168.1.82" },
  ];
  for (const { id, name, ip } of more) s.devices.push(configuredFrom({ ...CLOUD_DEVICES[0], id, name, lan: null }, ip));
  s.keyRejected = [];
  return s;
}

const SCENARIOS: Record<Scenario, () => MockState> = { fresh: freshState, configured: configuredState, four: fourState };

export const isScenario = (name: string | null): name is Scenario => name !== null && Object.hasOwn(SCENARIOS, name);

const ALL_STEPS: Record<string, OnboardingStep> = {
  welcome: "welcome",
  permissions: "permissions",
  tuya_account: "tuya_account",
  cloud_project: "cloud_project",
  link_app: "link_app",
  credentials: "credentials",
  choose_devices: "choose_devices",
  test_devices: "test_devices",
  zones: "zones",
  calibrate: "calibrate",
  done: "done",
};

/** Lo que el mock mira de un archivo de respaldo. */
interface MockBackup {
  format: string;
  version: number;
  created_at: string;
  app_version: string;
  devices: Pick<ConfiguredDevice_Serialize, "id" | "name" | "version" | "zones" | "overrides">[];
}

/** Copia profunda que también acepta los proxies reactivos de Svelte (como el IPC real, vía JSON). */
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

export function err(code: ErrorCode, actions: FixAction[], params: Record<string, string> = {}): UserFacingError {
  return { code, title: `error.${code}.title`, detail: `error.${code}.detail`, params, actions };
}

// ---------- Simulación de la sesión ----------

interface DeviceSim {
  state: DeviceSessionState;
  health: LinkHealth_Serialize;
  sent: { h: number; s: number; v: number } | null;
  manual: ManualSet | null;
}

class MockBackend {
  state: MockState;
  session: SessionState = "idle";
  sims = new Map<string, DeviceSim>();
  calibration: CalibrationState | null = null;
  log: LogLine[] = seedLog(Date.now());
  fast: boolean;
  autostart = false;
  updater: MockUpdater;
  /** Entre `update_prepare` y `update_abort` el efecto no se puede arrancar (como en la app). */
  private updating = false;
  /** La página pidió que cerrar la ventana no la destruya (`ui_hold`). */
  held = false;
  /** Se reparó el permiso de pantalla y el aviso de reabrir la app no se descartó. */
  private repaired = false;
  private timers: ReturnType<typeof setInterval>[] = [];
  private videoT = 0;
  private sceneHue = 18;
  private nextCut = 5;

  constructor(scenario: Scenario | null, fast: boolean, update: string | null) {
    this.fast = fast;
    this.updater = new MockUpdater(update, (ms) => this.wait(ms));
    this.state = scenario ? this.reset(scenario) : (this.load() ?? freshState());
    this.persist();
  }

  reset(scenario: Scenario): MockState {
    this.stopTimers();
    this.session = "idle";
    this.sims.clear();
    this.state = SCENARIOS[scenario]();
    this.persist();
    return this.state;
  }

  private load(): MockState | null {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      return raw ? (JSON.parse(raw) as MockState) : null;
    } catch {
      return null;
    }
  }

  private persist(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state));
    } catch {
      // sin localStorage el mock igual funciona; solo no recuerda entre recargas
    }
  }

  wait(ms: number): Promise<void> {
    return new Promise((r) => setTimeout(r, this.fast ? Math.min(ms, 40) : ms));
  }

  private addLog(level: LogLine["level"], target: string, message: string): void {
    this.log.push({ at_ms: Date.now(), level, target, message });
    if (this.log.length > 400) this.log.splice(0, this.log.length - 400);
  }

  device(id: string): ConfiguredDevice_Serialize {
    const d = this.state.devices.find((x) => x.id === id);
    if (!d) throw err("device_not_found", [{ kind: "retry" }, { kind: "show_help", value: "device_troubleshooting" }], { device: id });
    return d;
  }

  /** Como la app: mientras un foco sigue la pantalla, nadie más lo toma. */
  private free(id: string): void {
    const d = this.state.devices.find((x) => x.id === id);
    if (this.session !== "idle" && d?.in_effect !== false) throw err("device_in_effect", [{ kind: "remove_from_effect" }], { device: id });
  }

  private keyError(d: ConfiguredDevice_Serialize): UserFacingError {
    return err("device_key_rejected", [{ kind: "renew_key" }, { kind: "show_help", value: "device_troubleshooting" }], { device: d.id, name: d.name });
  }

  // ---------- Ajustes ----------

  effective(): Settings {
    const profile = this.state.profiles.find((p) => p.id === this.state.activeProfile);
    return { ...this.state.global, ...(profile?.values ?? {}) } as Settings;
  }

  snapshot(): SettingsSnapshot_Serialize {
    return {
      catalog: CATALOG,
      effective: this.effective(),
      global: { ...this.state.global },
      devices: Object.entries(this.state.deviceOverrides).map(([device, overrides]) => ({ device, overrides })),
      active_profile: this.state.activeProfile,
    };
  }

  validate(id: SettingId, value: SettingValue): void {
    const def = CATALOG.find((d) => d.id === id);
    const bad = err("invalid_setting", [{ kind: "retry" }], { setting: id });
    if (!def) throw bad;
    if (def.kind.kind === "number") {
      if (typeof value !== "number" || !Number.isFinite(value)) throw bad;
      if (value < (def.kind.min ?? 0) - 1e-9 || value > (def.kind.max ?? 0) + 1e-9) throw bad;
    } else if (typeof value !== "string" || !def.kind.options.includes(value)) {
      throw bad;
    }
  }

  overridesFor(scope: SettingScope): SettingsOverride | null {
    if (scope.kind === "device") return (this.state.deviceOverrides[scope.id] ??= {});
    if (scope.kind === "profile") {
      const p = this.state.profiles.find((x) => x.id === scope.id);
      if (!p) throw err("invalid_setting", [{ kind: "retry" }], { setting: scope.id });
      return p.values;
    }
    return null;
  }

  syncDeviceOverrides(): void {
    for (const d of this.state.devices) d.overrides = { ...(this.state.deviceOverrides[d.id] ?? {}) };
  }

  settingFor(device: string, id: SettingId): number | string {
    const o = this.state.deviceOverrides[device]?.[id];
    return (o ?? this.effective()[id]) as number | string;
  }

  // ---------- Sesión ----------

  private simFor(id: string): DeviceSim {
    let sim = this.sims.get(id);
    if (!sim) {
      sim = {
        state: { kind: "idle" },
        health: { connected: false, rtt_ms: null, bytes_rx: 0, bytes_tx: 0, reconnects: 0, disconnects_last_hour: 0, last_disconnect: null, last_disconnect_at_ms: null },
        sent: null,
        manual: null,
      };
      if (id === IDS.living) {
        sim.health.disconnects_last_hour = 2;
        sim.health.reconnects = 2;
        sim.health.last_disconnect = { kind: "peer_closed" };
        sim.health.last_disconnect_at_ms = Date.now() - 1_216_000;
      }
      this.sims.set(id, sim);
    }
    return sim;
  }

  private emitHealth(id: string): void {
    const sim = this.simFor(id);
    const ev: DeviceHealthEvent_Serialize = { device: id, state: sim.state, health: { ...sim.health } };
    void emit(EVENT_DEVICE_HEALTH, ev);
  }

  private setSession(state: SessionState): void {
    this.session = state;
    void emit(EVENT_SESSION_STATE, { state });
  }

  /** Los focos que participan del efecto. */
  private inEffect(): ConfiguredDevice_Serialize[] {
    return this.state.devices.filter((d) => d.in_effect !== false);
  }

  async start(): Promise<SessionState> {
    if (this.session === "active" || this.session === "connecting") return this.session;
    if (this.updating) return "idle";
    if (this.state.devices.length === 0) {
      throw err("device_not_found", [{ kind: "show_help", value: "device_troubleshooting" }, { kind: "retry" }]);
    }
    if (this.state.permissions.screen_recording !== "granted") {
      // Como la app en macOS: además de abrir Ajustes, se puede reparar el permiso.
      throw {
        ...err("screen_permission_denied", [{ kind: "open_settings", value: "screen_recording" }, { kind: "repair_screen_permission" }]),
        detail: "error.screen_permission_denied.detail_macos",
      };
    }
    if (this.inEffect().length === 0) {
      throw err("no_zones", [{ kind: "show_help", value: "device_troubleshooting" }, { kind: "retry" }]);
    }
    this.setSession("connecting");
    for (const d of this.inEffect()) {
      this.simFor(d.id).state = { kind: "connecting" };
      this.emitHealth(d.id);
    }
    await this.wait(700);
    this.setSession("active");
    this.addLog("info", "openlight_core", `Sesión activa con ${this.inEffect().length} focos`);
    for (const d of this.inEffect()) this.take(d);
    this.startTimers();
    return this.session;
  }

  /** La sesión toma un foco: conecta, o falla si su clave no sirve. */
  private take(d: ConfiguredDevice_Serialize): void {
    const sim = this.simFor(d.id);
    if (this.state.keyRejected.includes(d.id)) {
      sim.state = { kind: "error", code: "device_key_rejected" };
      sim.health.connected = false;
      sim.health.last_disconnect = { kind: "key_rejected" };
      sim.health.last_disconnect_at_ms = Date.now();
      this.addLog("error", "tuya_device", `${d.name}: la clave fue rechazada (914)`);
      void emit(EVENT_ERROR_RAISED, this.keyError(d));
    } else {
      sim.state = { kind: "active" };
      sim.health.connected = true;
      sim.health.rtt_ms = 24;
    }
    this.emitHealth(d.id);
  }

  /** El foco sale del efecto: vuelve a su estado y se desconecta. */
  private release(id: string): void {
    const sim = this.simFor(id);
    sim.state = { kind: "idle" };
    sim.health.connected = false;
    sim.sent = null;
    this.emitHealth(id);
  }

  async setInEffect(id: string, enabled: boolean): Promise<null> {
    const d = this.device(id);
    await this.wait(200);
    d.in_effect = enabled;
    this.persist();
    void emit(EVENT_EFFECT_DEVICES, { excluded: this.state.devices.filter((x) => x.in_effect === false).map((x) => x.id) });
    if (this.session !== "idle") {
      if (enabled) this.take(d);
      else this.release(id);
      if (this.inEffect().length === 0) await this.stop();
    }
    return null;
  }

  async stop(): Promise<SessionState> {
    if (this.session === "idle") return "idle";
    this.stopTimers();
    this.setSession("restoring");
    await this.wait(500);
    for (const d of this.state.devices) this.release(d.id);
    this.addLog("info", "openlight_core", "Focos devueltos a su estado original");
    this.setSession("idle");
    return "idle";
  }

  stopTimers(): void {
    this.timers.forEach(clearInterval);
    this.timers = [];
  }

  private startTimers(): void {
    this.stopTimers();
    this.timers.push(setInterval(() => this.tickColor(), 125)); // 8 Hz por foco (≤10 Hz)
    this.timers.push(setInterval(() => this.tickHealth(), 2000));
    this.timers.push(setInterval(() => this.tickStats(), 1000));
  }

  /** "Video" sintético: escenas con cortes cada 4–8 s, deriva lenta del tono y respiración de la luz. */
  screenFor(device: ConfiguredDevice_Serialize): { h: number | null; s: number; luz: number } {
    const z = device.zones[0];
    const offset = z?.preset === "left" ? -22 : z?.preset === "right" ? 26 : 0;
    const drift = Math.sin(this.videoT / 3) * 14;
    const luz = 0.35 + 0.25 * Math.sin(this.videoT / 1.7) + 0.1 * Math.sin(this.videoT * 2.3);
    const dark = luz < 0.14;
    const grey = Math.sin(this.videoT / 11) > 0.93;
    const h = (((this.sceneHue + offset + drift) % 360) + 360) % 360;
    return { h: grey || dark ? null : h / 360, s: grey ? 0 : 0.55 + 0.3 * Math.abs(Math.sin(this.videoT / 2.9)), luz: Math.max(0.04, Math.min(1, luz)) };
  }

  private tickColor(): void {
    this.videoT += 0.125;
    if (this.videoT > this.nextCut) {
      this.sceneHue = (this.sceneHue + 90 + Math.random() * 180) % 360;
      this.nextCut = this.videoT + 4 + Math.random() * 4;
    }
    for (const d of this.inEffect()) {
      const sim = this.simFor(d.id);
      const screen = this.screenFor(d);
      if (sim.state.kind !== "active") {
        void emit(EVENT_COLOR_LIVE, { device: d.id, screen, sent: null });
        continue;
      }
      const sat = Number(this.settingFor(d.id, "saturation"));
      const bmin = Number(this.settingFor(d.id, "brightness_min"));
      const bmax = Number(this.settingFor(d.id, "brightness_max"));
      const smoothing = Math.max(1, Number(this.settingFor(d.id, "smoothing_ms")));
      const target = {
        h: screen.h ?? sim.sent?.h ?? 0,
        s: screen.h === null ? 0 : Math.min(1, screen.s * sat),
        v: Math.max(0.01, bmin + (bmax - bmin) * screen.luz),
      };
      if (!sim.sent) sim.sent = target;
      const k = 1 - Math.exp(-125 / smoothing);
      let dh = target.h - sim.sent.h;
      if (dh > 0.5) dh -= 1;
      if (dh < -0.5) dh += 1;
      sim.sent = { h: (sim.sent.h + dh * k + 1) % 1, s: sim.sent.s + (target.s - sim.sent.s) * k, v: sim.sent.v + (target.v - sim.sent.v) * k };
      sim.health.bytes_tx += 88;
      const sent: Hsv = { ...sim.sent };
      void emit(EVENT_COLOR_LIVE, { device: d.id, screen, sent });
    }
  }

  private tickHealth(): void {
    for (const d of this.inEffect()) {
      const sim = this.simFor(d.id);
      if (sim.state.kind === "active") {
        sim.health.rtt_ms = Math.round(18 + Math.random() * 28 + (d.version === "3.5" ? 6 : 0));
        sim.health.bytes_rx += 120 + Math.round(Math.random() * 40);
        // Cada tanto el foco cierra el socket, como en el log del script (272 cortes).
        if (d.id === IDS.living && Math.random() < 0.04) this.disconnect(d, { kind: "peer_closed" });
      }
      this.emitHealth(d.id);
    }
  }

  private disconnect(d: ConfiguredDevice_Serialize, reason: DisconnectReason): void {
    const sim = this.simFor(d.id);
    sim.state = { kind: "reconnecting" };
    sim.health.connected = false;
    sim.health.last_disconnect = reason;
    sim.health.last_disconnect_at_ms = Date.now();
    sim.health.disconnects_last_hour += 1;
    this.addLog("warn", "tuya_device", `${d.name}: el foco cerró el socket (EOF); reconecto`);
    this.emitHealth(d.id);
    setTimeout(() => {
      if (this.session !== "active") return;
      sim.state = { kind: "active" };
      sim.health.connected = true;
      sim.health.reconnects += 1;
      this.addLog("info", "tuya_device", `${d.name}: reconectado; vuelvo a poner switch=true, mode=colour`);
      this.emitHealth(d.id);
    }, this.fast ? 50 : 1500);
  }

  private tickStats(): void {
    const fps = Number(this.effective().capture_fps ?? 20);
    void emit(EVENT_PIPELINE_STATS, {
      capture_fps: Math.max(0, fps - Math.random() * 1.6),
      devices: this.inEffect().map((d) => {
        const active = this.simFor(d.id).state.kind === "active";
        const max = Number(this.settingFor(d.id, "max_sends_per_s"));
        return { device: d.id, sends_per_s: active ? Math.min(max, 3 + Math.random() * 4.5) : 0 };
      }),
    });
  }

  // ---------- Comandos ----------

  onboarding(): OnboardingState {
    return {
      step: this.state.onboarding.step,
      completed: [...this.state.onboarding.completed],
      has_credentials: this.state.credentials !== null,
      devices: this.state.devices.map((d) => d.id),
    };
  }

  async cloudValidate(c: CloudCredentialsInput) {
    await this.wait(900);
    const id = c.access_id.trim();
    const secret = c.access_secret.trim();
    const help = (topic: "cloud_project" | "data_center" | "link_smart_life" | "iot_core_trial"): FixAction => ({ kind: "show_help", value: topic });
    const portal: FixAction = { kind: "open_url", value: "https://iot.tuya.com/cloud/" };
    if (id.length < 10) throw err("cloud_invalid_token", [{ kind: "retry" }, portal, help("cloud_project")]);
    if (secret.length !== 32) throw err("cloud_invalid_signature", [portal, help("cloud_project")]);
    if (id.startsWith("expired")) throw err("cloud_trial_expired", [{ kind: "open_url", value: "https://iot.tuya.com/cloud/products" }, help("iot_core_trial")]);
    if (id.startsWith("nolink")) throw err("cloud_permission_denied", [help("link_smart_life"), help("data_center")]);
    if (c.region !== CORRECT_REGION) throw err("cloud_wrong_region", [{ kind: "retry" }, help("data_center")], { region: CORRECT_REGION });
    this.state.credentials = { access_id: id, region: c.region };
    this.persist();
    return { uid: "az1715903338472Kq9Dx", region: c.region };
  }

  async cloudDevices() {
    await this.wait(700);
    if (!this.state.credentials) throw err("cloud_invalid_token", [{ kind: "show_help", value: "cloud_project" }, { kind: "retry" }]);
    return CLOUD_DEVICES.map((d) => ({ ...d }));
  }

  async discover(timeoutS: number | null) {
    const found = CLOUD_DEVICES.flatMap((d) => (d.lan ? [d.lan] : []));
    const step = ((timeoutS ?? 3) * 1000) / (found.length + 1);
    for (const lan of found) {
      await this.wait(step);
      void emit(EVENT_DISCOVERY_FOUND, lan);
    }
    return found;
  }

  async deviceAdd(req: DeviceAddRequest_Deserialize) {
    await this.wait(400);
    const cloud = CLOUD_DEVICES.find((d) => d.id === req.id);
    if (!cloud) throw err("device_not_found", [{ kind: "retry" }, { kind: "show_help", value: "device_troubleshooting" }], { device: req.id });
    if (!cloud.is_light || !cloud.capabilities?.dp_colour) throw err("device_no_colour", [{ kind: "show_help", value: "device_troubleshooting" }], { name: cloud.name });
    const existing = this.state.devices.find((d) => d.id === req.id);
    if (existing) return existing;
    const dev = configuredFrom(cloud, req.ip ?? null);
    dev.name = req.name || cloud.name;
    // Con dos focos, el segundo arranca en la mitad derecha.
    if (this.state.devices.length === 1) {
      this.state.devices[0].zones = [zone("left")];
      dev.zones = [zone("right")];
    }
    this.state.devices.push(dev);
    this.persist();
    return dev;
  }

  async deviceUpdate(req: DeviceAddRequest_Deserialize) {
    await this.wait(200);
    const d = this.device(req.id);
    d.name = req.name.trim() || d.name;
    d.auto_ip = !req.ip;
    if (req.ip) {
      if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(req.ip)) throw err("device_not_found", [{ kind: "retry" }], { device: d.id, ip: req.ip });
      d.ip = req.ip;
    }
    this.persist();
    return d;
  }

  async deviceRemove(id: string) {
    await this.wait(200);
    this.free(id);
    this.state.devices = this.state.devices.filter((d) => d.id !== id);
    delete this.state.deviceOverrides[id];
    this.persist();
    return null;
  }

  async devicesReorder(ids: string[]) {
    await this.wait(150);
    const byId = new Map(this.state.devices.map((d) => [d.id, d]));
    if (new Set(ids).size !== ids.length || ids.length !== byId.size || !ids.every((id) => byId.has(id))) {
      throw err("device_order_mismatch", [{ kind: "retry" }]);
    }
    this.state.devices = ids.map((id) => byId.get(id)!);
    this.persist();
    return null;
  }

  async deviceTest(id: string) {
    const d = this.device(id);
    this.free(id);
    await this.wait(1200);
    if (this.state.keyRejected.includes(id)) throw this.keyError(d);
    return null;
  }

  async deviceManual(id: string, set: ManualSet) {
    const d = this.device(id);
    await this.wait(80);
    this.free(id);
    if (this.state.keyRejected.includes(id)) throw this.keyError(d);
    if (set.kind === "colour" && !d.capabilities.dp_colour) throw err("device_no_colour", [{ kind: "show_help", value: "device_troubleshooting" }], { name: d.name });
    this.simFor(id).manual = set;
    this.addLog("debug", "tuya_device", `${d.name}: control manual ${set.kind}`);
    return null;
  }

  async renewKey(id: string) {
    const d = this.device(id);
    await this.wait(1200);
    if (!this.state.credentials) throw err("cloud_invalid_token", [{ kind: "show_help", value: "cloud_project" }]);
    this.state.keyRejected = this.state.keyRejected.filter((x) => x !== id);
    this.state.noKey = this.withoutKey().filter((x) => x !== id);
    this.persist();
    this.addLog("info", "tuya_cloud", `${d.name}: clave local renovada desde la nube`);
    if (this.session === "active") {
      const sim = this.simFor(id);
      sim.state = { kind: "active" };
      sim.health.connected = true;
      sim.health.reconnects += 1;
      this.emitHealth(id);
    }
    return null;
  }

  zonesSet(id: string, zones: Zone[]) {
    this.device(id).zones = zones;
    this.persist();
    return null;
  }

  settingsSet(scope: SettingScope, id: SettingId, value: SettingValue) {
    this.validate(id, value);
    const target = this.overridesFor(scope);
    if (target) target[id] = value;
    else (this.state.global as Record<SettingId, SettingValue>)[id] = value;
    this.syncDeviceOverrides();
    this.persist();
    return this.snapshot();
  }

  settingsReset(scope: SettingScope, id: SettingId | null) {
    const target = this.overridesFor(scope);
    const defaults = defaultSettings() as Record<SettingId, SettingValue>;
    if (target) {
      if (id) delete target[id];
      else for (const k of Object.keys(target)) delete target[k as SettingId];
    } else if (id) (this.state.global as Record<SettingId, SettingValue>)[id] = defaults[id];
    else this.state.global = defaultSettings();
    this.syncDeviceOverrides();
    this.persist();
    return this.snapshot();
  }

  profilesCreate(name: string, from: string | null) {
    const source = this.state.profiles.find((p) => p.id === from);
    const profile: Profile_Serialize = {
      id: `custom-${Date.now().toString(36)}`,
      name: name.trim() || "Perfil",
      values: { ...(source?.values ?? {}) },
      shortcut: null,
      builtin: false,
    };
    this.state.profiles.push(profile);
    this.persist();
    return profile;
  }

  profilesUpdate(p: Profile_Deserialize) {
    const i = this.state.profiles.findIndex((x) => x.id === p.id);
    if (i < 0) throw err("storage", [{ kind: "retry" }]);
    const current = this.state.profiles[i];
    const updated: Profile_Serialize = { ...current, values: p.values, shortcut: p.shortcut ?? null, name: current.builtin ? current.name : p.name };
    this.state.profiles[i] = updated;
    this.persist();
    return updated;
  }

  profilesDelete(id: string) {
    const p = this.state.profiles.find((x) => x.id === id);
    if (p?.builtin) throw err("storage", [{ kind: "retry" }]);
    this.state.profiles = this.state.profiles.filter((x) => x.id !== id);
    if (this.state.activeProfile === id) this.state.activeProfile = null;
    this.persist();
    return null;
  }

  calibrationStart(device: string, display: string, quick: boolean): CalibrationState {
    this.device(device);
    this.free(device);
    const saved = this.state.calibrations[device] ?? [];
    const hues = quick ? QUICK_HUES : HUES;
    this.calibration = {
      device,
      display,
      index: 0,
      points: hues.map((h) => saved.find((p) => p.screen_hue_deg === h) ?? { screen_hue_deg: h, tono_deg: h, sat: 1 }),
    };
    return clone(this.calibration);
  }

  calibrationSet(index: number, tono: number | null, sat: number | null): CalibrationState {
    const c = this.calibration;
    if (!c) throw err("internal", [{ kind: "retry" }]);
    const p = c.points[index];
    if (!p) throw err("invalid_setting", [{ kind: "retry" }], { setting: "calibration" });
    const base = p.screen_hue_deg ?? 0;
    if (tono !== null) p.tono_deg = Math.max(base - 90, Math.min(base + 90, tono));
    if (sat !== null) p.sat = Math.max(0.3, Math.min(1.5, Math.round(sat * 100) / 100));
    c.index = index;
    return clone(c);
  }

  calibrationSave() {
    const c = this.calibration;
    if (!c) return null;
    const merged = [...(this.state.calibrations[c.device] ?? [])].filter((p) => !c.points.some((q) => q.screen_hue_deg === p.screen_hue_deg));
    this.state.calibrations[c.device] = [...merged, ...c.points].sort((a, b) => (a.screen_hue_deg ?? 0) - (b.screen_hue_deg ?? 0));
    this.device(c.device).calibrated = true;
    this.calibration = null;
    this.persist();
    return null;
  }

  // ---------- Respaldo ----------

  async backupExport(): Promise<string> {
    await this.wait(300);
    return `~/Downloads/openlight-backup-${new Date().toISOString().slice(0, 10)}.json`;
  }

  /** Como `openlight_core::backup::parse`, con lo justo: qué es y de qué versión. */
  private parseBackup(content: string): MockBackup {
    const invalid = () => err("backup_invalid", [{ kind: "retry" }]);
    let file: Partial<MockBackup>;
    try {
      file = JSON.parse(content) as Partial<MockBackup>;
    } catch {
      throw invalid();
    }
    if (file?.format !== "openlight-backup" || typeof file.version !== "number") throw invalid();
    if (file.version !== 1) throw err("backup_unsupported_version", [{ kind: "retry" }], { found: String(file.version), supported: "1" });
    if (!Array.isArray(file.devices)) throw invalid();
    return file as MockBackup;
  }

  backupInspect(content: string): BackupSummary {
    const file = this.parseBackup(content);
    const here = (id: string) => this.state.devices.some((d) => d.id === id);
    const brief = (list: MockBackup["devices"]) => list.map((d) => ({ id: d.id, name: d.name }));
    return {
      created_at: file.created_at,
      app_version: file.app_version,
      new_devices: brief(file.devices.filter((d) => !here(d.id))),
      existing_devices: brief(file.devices.filter((d) => here(d.id))),
    };
  }

  async backupImport(content: string, duplicates: DuplicatePolicy): Promise<ImportReport> {
    const file = this.parseBackup(content);
    await this.wait(400);
    const imported: string[] = [];
    for (const d of file.devices) {
      const current = this.state.devices.find((x) => x.id === d.id);
      if (current && duplicates === "skip") continue;
      const fields = { name: d.name, version: d.version, zones: d.zones ?? [], overrides: d.overrides ?? {} };
      if (current) Object.assign(current, fields);
      else {
        const cloud = CLOUD_DEVICES.find((c) => c.id === d.id) ?? CLOUD_DEVICES[0];
        this.state.devices.push({ ...configuredFrom(cloud, null), id: d.id, ip: null, ...fields });
        (this.state.noKey ??= []).push(d.id);
      }
      this.state.deviceOverrides[d.id] = { ...fields.overrides };
      imported.push(d.id);
    }
    this.persist();
    return { devices: imported, calibrations: 0, settings_applied: duplicates === "replace", warnings: [] };
  }

  withoutKey(): string[] {
    return (this.state.noKey ?? []).filter((id) => this.state.devices.some((d) => d.id === id));
  }

  async fetchMissingKeys(): Promise<MissingKeysReport> {
    const pending = this.withoutKey();
    if (pending.length === 0) return { fetched: [], not_found: [] };
    await this.wait(900);
    if (!this.state.credentials) throw err("cloud_no_credentials", [{ kind: "show_help", value: "cloud_project" }]);
    const inAccount = (id: string) => CLOUD_DEVICES.some((c) => c.id === id);
    this.state.noKey = pending.filter((id) => !inAccount(id));
    this.persist();
    return { fetched: pending.filter(inAccount), not_found: [...this.state.noKey] };
  }

  async importLegacy(folder: string) {
    await this.wait(900);
    if (!/open\s?rgb|devices\.json/i.test(folder)) {
      throw err("import_invalid", [{ kind: "retry" }, { kind: "show_help", value: "tuya_account" }], { folder });
    }
    const configured = configuredState();
    this.state.devices = configured.devices;
    this.state.deviceOverrides = configured.deviceOverrides;
    this.state.calibrations = configured.calibrations;
    this.state.keyRejected = [];
    this.state.credentials = configured.credentials;
    this.persist();
    return { devices: configured.devices.map((d) => d.id), calibrations: 1, settings_applied: true, warnings: ["import.warning.calibration_single_device"] };
  }

  permissions() {
    const p = this.state.permissions;
    return [
      { kind: "screen_recording" as const, state: p.screen_recording, pane: "screen_recording" as const, needs_restart: true },
      { kind: "local_network" as const, state: p.local_network, pane: "local_network" as const, needs_restart: false },
    ];
  }

  async openSystemSettings(pane: SystemPane) {
    // Simula que la persona activa el permiso en Ajustes del Sistema al rato.
    setTimeout(() => {
      if (pane in this.state.permissions) this.state.permissions[pane as PermissionKind] = "granted";
      this.persist();
    }, this.fast ? 60 : 1800);
    return null;
  }

  async repairScreenPermission() {
    // Como `tccutil reset` seguido del pedido: queda sin dar hasta que se vuelva a activar.
    await this.wait(300);
    this.state.permissions.screen_recording = "denied";
    this.persist();
    this.repaired = true;
    return null;
  }

  /** Para el panel de escenarios del mock: inyecta una falla visible. */
  inject(kind: "key_rejected" | "disconnect" | "no_display"): void {
    const d = this.state.devices[0];
    if (!d) return;
    if (kind === "key_rejected") {
      if (!this.state.keyRejected.includes(d.id)) this.state.keyRejected.push(d.id);
      const sim = this.simFor(d.id);
      sim.state = { kind: "error", code: "device_key_rejected" };
      sim.health.connected = false;
      this.emitHealth(d.id);
      void emit(EVENT_ERROR_RAISED, this.keyError(d));
    } else if (kind === "disconnect") {
      this.disconnect(d, { kind: "timeout" });
    } else {
      void emit(EVENT_ERROR_RAISED, err("no_display", [{ kind: "retry" }, { kind: "show_help", value: "permissions" }]));
    }
    this.persist();
  }

  /** Como el backend con una ventana nueva: el estado del efecto y de cada foco. */
  private replay(): void {
    if (this.session === "idle") return;
    void emit(EVENT_SESSION_STATE, { state: this.session });
    for (const d of this.inEffect()) this.emitHealth(d.id);
  }

  async handle(cmd: string, a: Record<string, any>): Promise<unknown> {
    switch (cmd) {
      case "onboarding_state":
        return this.onboarding();
      case "onboarding_save":
        this.state.onboarding = { step: a.step, completed: a.completed };
        this.persist();
        return this.onboarding();
      case "check_permissions":
        return this.permissions();
      case "open_system_settings":
        return this.openSystemSettings(a.pane);
      case "repair_screen_permission":
        return this.repairScreenPermission();
      case "cloud_validate":
        return this.cloudValidate(a.credentials);
      case "cloud_list_devices":
        return this.cloudDevices();
      case "discover_lan":
        return this.discover(a.timeoutS);
      case "devices_list":
        return clone(this.state.devices);
      case "device_add":
        return this.deviceAdd(a.request);
      case "device_update":
        return this.deviceUpdate(a.request);
      case "device_remove":
        return this.deviceRemove(a.device);
      case "devices_reorder":
        return this.devicesReorder(a.ids);
      case "device_test":
        return this.deviceTest(a.device);
      case "device_manual":
        return this.deviceManual(a.device, a.set);
      case "device_set_in_effect":
        return this.setInEffect(a.device, a.enabled);
      case "renew_key":
        return this.renewKey(a.device);
      case "displays_list":
        return DISPLAYS.map((d) => ({ ...d }));
      case "display_preview":
        return previewImage(a.display, this.videoT);
      case "zones_set":
        return this.zonesSet(a.device, a.zones);
      case "settings_get":
        return this.snapshot();
      case "settings_set":
        return this.settingsSet(a.scope, a.id, a.value);
      case "settings_reset":
        return this.settingsReset(a.scope, a.id);
      case "profiles_list":
        return clone(this.state.profiles);
      case "profiles_create":
        return this.profilesCreate(a.name, a.from);
      case "profiles_update":
        return this.profilesUpdate(a.profile);
      case "profiles_delete":
        return this.profilesDelete(a.id);
      case "profiles_activate":
        this.state.activeProfile = a.id ?? null;
        this.persist();
        return null;
      case "session_state":
        return this.session;
      case "session_start":
        this.updater.calls.push("session_start");
        return this.start();
      case "session_stop":
        return this.stop();
      case "calibration_start":
        return this.calibrationStart(a.device, a.display, a.quick);
      case "calibration_set":
        return this.calibrationSet(a.index, a.tonoDeg, a.sat);
      case "calibration_save":
        return this.calibrationSave();
      case "calibration_cancel":
        this.calibration = null;
        return null;
      case "backup_export":
        return this.backupExport();
      case "backup_inspect":
        return this.backupInspect(a.content);
      case "backup_import":
        return this.backupImport(a.content, a.duplicates);
      case "devices_without_key":
        return this.withoutKey();
      case "fetch_missing_keys":
        return this.fetchMissingKeys();
      case "import_legacy":
        return this.importLegacy(a.folder);
      case "diagnostics_log":
        return this.log.slice(-(a.limit ?? 200));
      case "diagnostics_export":
        await this.wait(800);
        return a.destination ?? "~/Desktop/openlight-diagnostico.zip";
      case "update_prepare":
        this.updater.calls.push("update_prepare");
        await this.stop();
        this.updating = true;
        return null;
      case "update_abort":
        this.updater.calls.push("update_abort");
        this.updating = false;
        return null;
      case "ui_ready":
        this.held = false;
        this.replay();
        return this.repaired;
      case "screen_repair_dismiss":
        this.repaired = false;
        return null;
      case "ui_hold":
        this.held = a.hold;
        return null;
      // Plugins de Tauri que usa la UI (no son parte de ipc.rs).
      case "plugin:app|version":
        return MOCK_VERSION;
      case "plugin:updater|check":
        return this.updater.check();
      case "plugin:updater|download":
        return this.updater.download(a.onEvent);
      case "plugin:updater|install":
        return this.updater.install();
      case "plugin:process|restart":
        return this.updater.restart();
      case "plugin:resources|close":
        return null;
      case "plugin:autostart|is_enabled":
        return this.autostart;
      case "plugin:autostart|enable":
        this.autostart = true;
        return null;
      case "plugin:autostart|disable":
        this.autostart = false;
        return null;
      case "plugin:opener|open_url":
        window.open(a.url, "_blank", "noopener,noreferrer");
        return null;
      default:
        throw err("not_implemented", [{ kind: "retry" }], { command: cmd });
    }
  }
}

/** PNG en base64 de un "cuadro de película" sintético (atardecer a la izquierda, mar a la derecha). */
function previewImage(display: string, t: number) {
  const info = DISPLAYS.find((d) => d.id === display) ?? DISPLAYS[0];
  const width = 320;
  const height = Math.round((width * info.height) / info.width);
  let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
  if (typeof document !== "undefined") {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (ctx) {
      const shift = Math.sin(t / 4) * 20;
      const sky = ctx.createLinearGradient(0, 0, width, height);
      sky.addColorStop(0, `hsl(${22 + shift} 85% 55%)`);
      sky.addColorStop(0.45, `hsl(${340 + shift} 60% 42%)`);
      sky.addColorStop(1, `hsl(${195 + shift} 70% 32%)`);
      ctx.fillStyle = sky;
      ctx.fillRect(0, 0, width, height);
      ctx.fillStyle = "hsl(45 100% 70%)";
      ctx.beginPath();
      ctx.arc(width * 0.28, height * 0.55, height * 0.16, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "hsl(210 50% 12%)";
      ctx.fillRect(0, height * 0.68, width, height * 0.32);
      ctx.fillStyle = "rgba(0,0,0,.9)";
      ctx.fillRect(0, 0, width, height * 0.08);
      ctx.fillRect(0, height * 0.92, width, height * 0.08);
      png = canvas.toDataURL("image/png").split(",")[1] ?? png;
    }
  }
  return { display: info.id, width, height, png_base64: png };
}

let backend: MockBackend | null = null;

export function installMocks(opts: { scenario?: Scenario | null; fast?: boolean; update?: string | null } = {}): MockBackend {
  backend = new MockBackend(opts.scenario ?? null, opts.fast ?? false, opts.update ?? null);
  const b = backend;
  mockIPC((cmd, args) => b.handle(cmd, (args ?? {}) as Record<string, unknown>), { shouldMockEvents: true });
  return b;
}

export function mockBackend(): MockBackend | null {
  return backend;
}

export type { MockBackend };
