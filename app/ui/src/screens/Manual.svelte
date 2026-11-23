<script lang="ts">
  import { untrack } from "svelte";
  // E. Control manual: color, blanco con temperatura, brillo y escenas del foco si las tiene.
  import type { ManualSet, UserFacingError } from "../lib/bindings";
  import { app, toggleSession } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { hsvCss } from "../lib/format";
  import { HUE_KEYS } from "../lib/hue";
  import scenesFile from "../content/scenes.json";
  import ErrorCard from "../lib/components/ErrorCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Switch from "../lib/components/Switch.svelte";

  let { deviceId }: { deviceId: string | null } = $props();
  let selected = $state(untrack(() => deviceId) ?? app.devices[0]?.id ?? "");
  const device = $derived(app.devices.find((d) => d.id === selected));
  const caps = $derived(device?.capabilities);
  const scenes = (scenesFile as { scenes: { id: string; name: Record<string, string>; value: string }[] }).scenes;

  let power = $state(true);
  let mode = $state<"colour" | "white">("colour");
  let h = $state(0.08);
  let s = $state(0.85);
  let v = $state(0.9);
  let warm = $state(0.3);
  let wb = $state(0.8);
  let error = $state<UserFacingError | null>(null);
  let last: ManualSet | null = null;

  $effect(() => {
    if (caps && !caps.dp_colour) mode = "white";
  });

  // Un envío por vez y el último gana (como la coalescencia del actor).
  let busy = false;
  let queued: ManualSet | null = null;
  async function send(set: ManualSet) {
    if (!device) return;
    last = set;
    queued = set;
    if (busy) return;
    busy = true;
    while (queued) {
      const job = queued;
      queued = null;
      const r = await attempt(() => commands.deviceManual(device.id, job));
      error = r.ok ? null : r.error;
    }
    busy = false;
  }

  const colour = () => send({ kind: "colour", value: { h, s, v } });
  const white = () => send({ kind: "white", value: { brightness: wb, temperature: 1 - warm } });
  const swatch = $derived(mode === "colour" ? hsvCss({ h, s, v }) : `color-mix(in srgb, #ffb46b ${Math.round(warm * 100)}%, #dfeaff)`);
  const running = $derived(app.session === "active");
</script>

<div class="page">
  <header>
    <span class="eyebrow">{t("nav.manual")}</span>
    <h1>{t("manual.title")}</h1>
  </header>

  {#if !device}
    <div class="card empty"><p>{t("manual.no_devices")}</p><button type="button" class="btn primary" onclick={() => go("onboarding")}>{t("dashboard.empty_cta")}</button></div>
  {:else}
    <div class="bar">
      <div class="field">
        <label for="manual-device">{t("manual.device")}</label>
        <select id="manual-device" bind:value={selected}>{#each app.devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}</select>
      </div>
      <div class="power">
        <Switch checked={power} label={t("manual.power")} onchange={(p) => { power = p; void send({ kind: "power", value: p }); }} />
        <span>{t("manual.power")}</span>
      </div>
    </div>

    {#if running}
      <div class="warn card"><Icon name="alert" size={18} /><p>{t("manual.session_running")}</p><button type="button" class="btn small" onclick={toggleSession}>{t("session.stop")}</button></div>
    {/if}
    {#if error}<ErrorCard {error} retry={() => last && send(last)} />{/if}

    <div class="layout">
      <div class="lamp" aria-hidden="true" style:--c={power ? swatch : "var(--surface-3)"} class:off={!power}><span></span></div>

      <div class="controls">
        <div class="tabs" role="radiogroup" aria-label={t("manual.mode")}>
          <label class:on={mode === "colour"} class:disabled={!caps?.dp_colour}><input type="radio" bind:group={mode} value="colour" disabled={!caps?.dp_colour} />{t("manual.colour")}</label>
          <label class:on={mode === "white"} class:disabled={!caps?.dp_bright}><input type="radio" bind:group={mode} value="white" disabled={!caps?.dp_bright} />{t("manual.white")}</label>
        </div>

        {#if mode === "colour"}
          <div class="hues" role="group" aria-label={t("manual.quick_colours")}>
            {#each HUE_KEYS as k, i (k)}
              <button type="button" style:--c="hsl({i * 30} 100% 50%)" aria-label={t(`hue.${k}`)} aria-pressed={Math.round(h * 12) % 12 === i} onclick={() => { h = i / 12; s = 1; void colour(); }}></button>
            {/each}
          </div>
          <div class="slider hue"><label for="m-h">{t("manual.hue")}</label><output class="tabular" for="m-h">{Math.round(h * 360)}°</output>
            <input id="m-h" type="range" min="0" max="1" step="0.0028" bind:value={h} oninput={colour} aria-valuetext="{Math.round(h * 360)}°" /></div>
          <div class="slider"><label for="m-s">{t("manual.saturation")}</label><output class="tabular" for="m-s">{Math.round(s * 100)} %</output>
            <input id="m-s" type="range" min="0" max="1" step="0.01" bind:value={s} oninput={colour} style:--fill="{s * 100}%" aria-valuetext="{Math.round(s * 100)} %" /></div>
          <div class="slider"><label for="m-v">{t("manual.brightness")}</label><output class="tabular" for="m-v">{Math.round(v * 100)} %</output>
            <input id="m-v" type="range" min="0.01" max="1" step="0.01" bind:value={v} oninput={colour} style:--fill="{v * 100}%" aria-valuetext="{Math.round(v * 100)} %" /></div>
        {:else}
          {#if caps?.dp_temp}
            <div class="slider temp"><label for="m-t">{t("manual.temperature")}</label><output class="tabular" for="m-t">{warm > 0.5 ? t("manual.warm") : t("manual.cool")}</output>
              <input id="m-t" type="range" min="0" max="1" step="0.01" bind:value={warm} oninput={white} aria-valuetext={warm > 0.5 ? t("manual.warm") : t("manual.cool")} /></div>
          {/if}
          <div class="slider"><label for="m-wb">{t("manual.brightness")}</label><output class="tabular" for="m-wb">{Math.round(wb * 100)} %</output>
            <input id="m-wb" type="range" min="0.01" max="1" step="0.01" bind:value={wb} oninput={white} style:--fill="{wb * 100}%" aria-valuetext="{Math.round(wb * 100)} %" /></div>
        {/if}

        {#if caps?.modes.includes("scene")}
          <section class="scenes" aria-labelledby="scenes-title">
            <h2 id="scenes-title">{t("manual.scenes")}</h2>
            {#if scenes.length}
              <div class="row">{#each scenes as sc (sc.id)}<button type="button" class="btn small" onclick={() => send({ kind: "scene", value: sc.value })}>{sc.name[t("app.lang")] ?? sc.id}</button>{/each}</div>
            {:else}
              <p class="muted">{t("manual.scenes_from_app")}</p>
            {/if}
          </section>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    display: grid;
    gap: 1.3rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 1000px;
  }
  .bar {
    display: flex;
    gap: 1.5rem;
    align-items: end;
    flex-wrap: wrap;
  }
  .bar .field {
    min-width: 240px;
  }
  .power {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-weight: 700;
  }
  .warn {
    display: flex;
    gap: 0.7rem;
    align-items: center;
    padding: 0.7rem 1rem;
    color: var(--warn);
  }
  .warn p {
    flex: 1;
    color: var(--text);
  }
  .layout {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: 2.4rem;
    align-items: start;
  }
  .lamp {
    position: relative;
    aspect-ratio: 1;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 45%, var(--c), color-mix(in srgb, var(--c) 30%, transparent) 55%, transparent 70%);
    transition: background 200ms;
  }
  .lamp span {
    position: absolute;
    inset: 32%;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 60px 10px var(--c);
  }
  .lamp.off span {
    box-shadow: none;
  }
  .controls {
    display: grid;
    gap: 1.1rem;
  }
  .tabs {
    display: inline-flex;
    justify-self: start;
    padding: 4px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .tabs label {
    position: relative;
    padding: 0.45em 1.1em;
    border-radius: 999px;
    font-weight: 700;
    cursor: pointer;
    color: var(--muted);
  }
  .tabs label.disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .tabs input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
  }
  .tabs label.on {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .tabs label:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .hues {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: 6px;
  }
  .hues button {
    aspect-ratio: 1;
    border-radius: 50%;
    border: 2px solid transparent;
    background: var(--c);
    cursor: pointer;
  }
  .hues button[aria-pressed="true"] {
    border-color: var(--text);
    box-shadow: 0 0 14px var(--c);
  }
  .slider {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.15rem 1rem;
  }
  .slider label {
    font-weight: 700;
  }
  .slider output {
    color: var(--muted);
  }
  .slider input {
    grid-column: 1 / -1;
  }
  .hue input {
    --track: linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
  }
  .temp input {
    --track: linear-gradient(90deg, #dfeaff, #ffb46b);
  }
  .scenes {
    display: grid;
    gap: 0.5rem;
    padding-top: 1rem;
    border-top: 1px solid var(--line);
  }
  .scenes h2 {
    font-size: 1.1rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .empty {
    display: grid;
    gap: 0.8rem;
    justify-items: start;
    padding: 1.4rem;
  }
  @media (max-width: 760px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .lamp {
      width: 180px;
    }
  }
</style>
