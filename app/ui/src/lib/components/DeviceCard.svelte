<script lang="ts">
  // Tarjeta de un foco en el panel: estado, color de pantalla vs. enviado, latencia, cortes y brillo rápido.
  import type { ConfiguredDevice_Serialize, UserFacingError } from "../bindings";
  import { app, dismiss, raise, refreshSettings } from "../app.svelte";
  import { attempt, commands } from "../ipc";
  import { go } from "../router.svelte";
  import { i18n, t } from "../i18n/index.svelte";
  import { formatAgo } from "../format";
  import Eclipse from "./Eclipse.svelte";
  import ErrorCard from "./ErrorCard.svelte";
  import Icon from "./Icon.svelte";

  let { device }: { device: ConfiguredDevice_Serialize } = $props();

  const health = $derived(app.health[device.id]);
  const live = $derived(app.live[device.id]);
  const kind = $derived(health?.state.kind ?? (app.session === "idle" ? "idle" : "connecting"));
  const off = $derived(kind !== "active");
  const raised = $derived(app.errors.find((e) => e.params.device === device.id));
  const error = $derived.by<UserFacingError | null>(() => {
    if (raised) return raised;
    if (health?.state.kind !== "error") return null;
    const code = health.state.code;
    return {
      code,
      title: `error.${code}.title`,
      detail: `error.${code}.detail`,
      params: { device: device.id, name: device.name },
      actions: code === "device_key_rejected" ? [{ kind: "renew_key" }, { kind: "show_help", value: "device_troubleshooting" }] : [{ kind: "show_help", value: "device_troubleshooting" }],
    };
  });

  // Brillo rápido = brightness_max de este foco (pisa el global).
  const bmax = $derived(Number(device.overrides.brightness_max ?? app.settings?.effective.brightness_max ?? 1));
  let local = $state<number | null>(null);
  const shown = $derived(local ?? bmax);
  let timer: ReturnType<typeof setTimeout> | undefined;
  function brightness(v: number) {
    local = v;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const r = await attempt(() => commands.settingsSet({ kind: "device", id: device.id }, "brightness_max", v));
      if (!r.ok) raise(r.error);
      else {
        app.settings = r.value;
        device.overrides = r.value.devices.find((d) => d.device === device.id)?.overrides ?? device.overrides;
      }
      local = null;
    }, 150);
  }

  const reason = $derived(health?.health.last_disconnect);
  const chipClass = $derived(kind === "active" ? "ok" : kind === "error" ? "danger" : kind === "reconnecting" || kind === "connecting" ? "warn" : "");
</script>

<article class="card device" data-state={kind} aria-labelledby="dev-{device.id}-name">
  <header>
    <div>
      <h3 id="dev-{device.id}-name">{device.name}</h3>
      <p class="muted mono small">{device.ip ?? t("devices.auto_ip")} · v{device.version}{device.calibrated ? "" : ` · ${t("devices.not_calibrated")}`}</p>
    </div>
    <span class="chip {chipClass}" role="status">{t(`device_state.${kind}`)}</span>
  </header>

  <div class="stage">
    <Eclipse screen={live?.screen} sent={live?.sent} {off} size={132} />
    <dl class="stats">
      <div><dt>{t("dashboard.latency")}</dt><dd class="tabular">{health?.health.rtt_ms != null && !off ? `${health.health.rtt_ms} ms` : "—"}</dd></div>
      <div><dt>{t("dashboard.drops")}</dt><dd class="tabular">{health?.health.disconnects_last_hour ?? 0}</dd></div>
      <div class="wide">
        <dt>{t("dashboard.last_drop")}</dt>
        <dd>{reason ? `${t(`disconnect.${reason.kind}`)} · ${formatAgo(health!.health.last_disconnect_at_ms ?? Date.now(), i18n.locale)}` : t("dashboard.no_drops")}</dd>
      </div>
    </dl>
  </div>

  <div class="legend muted small" aria-hidden="true"><span><i class="ring"></i>{t("dashboard.screen")}</span><span><i class="core"></i>{t("dashboard.sent")}</span></div>

  {#if error}
    <ErrorCard {error} onresolved={() => raised && dismiss(raised.uid)} />
  {/if}

  <div class="bright">
    <label for="bright-{device.id}">{t("dashboard.brightness")}</label>
    <input id="bright-{device.id}" type="range" min="0" max="1" step="0.01" value={shown} style:--fill="{shown * 100}%" aria-valuetext="{Math.round(shown * 100)} %" oninput={(e) => brightness(Number(e.currentTarget.value))} />
    <output class="tabular" for="bright-{device.id}">{Math.round(shown * 100)} %</output>
  </div>

  <footer>
    <button type="button" class="btn small" onclick={() => go("manual", device.id)}><Icon name="bulb" size={16} />{t("dashboard.manual")}</button>
    <button type="button" class="btn small" onclick={() => go("calibrate", device.id)}><Icon name="palette" size={16} />{t("dashboard.calibrate")}</button>
    <button type="button" class="btn ghost small" onclick={async () => { await refreshSettings(); go("settings", device.id); }}><Icon name="sliders" size={16} />{t("dashboard.device_settings")}</button>
  </footer>
</article>

<style>
  .device {
    display: grid;
    gap: 1rem;
    padding: 1.2rem 1.3rem;
    transition: border-color 300ms;
  }
  .device[data-state="error"] {
    border-color: color-mix(in srgb, var(--danger) 40%, var(--line));
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
  }
  h3 {
    font-size: 1.35rem;
  }
  .small {
    font-size: 0.8rem;
  }
  .stage {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 1.6rem;
    align-items: center;
    padding: 0.6rem 0.2rem;
  }
  .stats {
    margin: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.9rem 1rem;
  }
  .stats .wide {
    grid-column: 1 / -1;
  }
  dt {
    font: 500 0.7rem var(--font-mono);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
  }
  dd {
    margin: 0.15rem 0 0;
    font-size: 1.05rem;
  }
  .stats .wide dd {
    font-size: 0.92rem;
  }
  .legend {
    display: flex;
    gap: 1rem;
  }
  .legend span {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
  }
  .legend i {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .legend .ring {
    border: 3px solid var(--muted);
  }
  .legend .core {
    background: var(--muted);
  }
  .bright {
    display: grid;
    grid-template-columns: auto 1fr 3.5rem;
    gap: 0.8rem;
    align-items: center;
  }
  .bright label {
    font-weight: 700;
    font-size: 0.9rem;
  }
  .bright output {
    text-align: right;
    color: var(--accent);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    padding-top: 0.8rem;
    border-top: 1px solid var(--line);
  }
</style>
