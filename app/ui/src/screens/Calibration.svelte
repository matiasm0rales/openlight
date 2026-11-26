<script lang="ts">
  import { untrack } from "svelte";
  // C. Calibración (reemplaza a calibrar.py): toda la ventana muestra el tono de pantalla y con los
  // controles se mueve el foco en vivo hasta que se parezcan. Mismas teclas que el script:
  // ←/→ tono (±2°), ↑/↓ saturación (±5 %), Enter = siguiente. 12 tonos, o 6 en modo rápido.
  import type { CalibrationState, UserFacingError } from "../lib/bindings";
  import { app, raise, refreshDevices } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { go } from "../lib/router.svelte";
  import { i18n, t } from "../lib/i18n/index.svelte";
  import { hueKey } from "../lib/hue";
  import { hueCss } from "../lib/format";
  import { wizard } from "../lib/wizard.svelte";
  import ErrorCard from "../lib/components/ErrorCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import ABTest from "../lib/components/ABTest.svelte";

  let { deviceId }: { deviceId: string | null } = $props();

  const [paramId, flag] = (untrack(() => deviceId) ?? "").split("~");
  let quick = $state(flag === "quick");
  const device = $derived(app.devices.find((d) => d.id === paramId) ?? app.devices[0]);
  let cal = $state<CalibrationState | null>(null);
  let error = $state<UserFacingError | null>(null);
  let done = $state<Set<number>>(new Set());
  let mode = $state<"hues" | "ab">("hues");
  let status = $state("");
  let saving = $state(false);

  async function start() {
    if (!device) return;
    error = null;
    const displays = await attempt(() => commands.displaysList());
    if (!displays.ok) return (error = displays.error);
    const display = device.zones[0]?.display ?? displays.value.find((d) => !d.builtin)?.id ?? displays.value[0]?.id;
    if (!display) return (error = { code: "no_display", title: "error.no_display.title", detail: "error.no_display.detail", params: {}, actions: [{ kind: "retry" }] });
    const r = await attempt(() => commands.calibrationStart(device.id, display, quick));
    if (!r.ok) return (error = r.error);
    cal = r.value;
    done = new Set();
  }

  $effect(() => {
    void start();
  });

  const point = $derived(cal?.points[cal.index]);
  const base = $derived(point?.screen_hue_deg ?? 0);
  const tono = $derived(point?.tono_deg ?? base);
  const sat = $derived(point?.sat ?? 1);
  const total = $derived(cal?.points.length ?? 0);

  // Envío en vivo con un solo pedido en vuelo, como `enviar()` de calibrar.py.
  let inFlight = false;
  let pending: { index: number; tono: number; sat: number } | null = null;
  async function send(index: number, tonoDeg: number, s: number) {
    if (!cal) return;
    const p = cal.points[index];
    p.tono_deg = tonoDeg;
    p.sat = s;
    done.add(index);
    done = new Set(done);
    pending = { index, tono: tonoDeg, sat: s };
    if (inFlight) return;
    while (pending) {
      const job = pending;
      pending = null;
      inFlight = true;
      const r = await attempt(() => commands.calibrationSet(job.index, job.tono, job.sat));
      if (!r.ok) {
        error = r.error;
        pending = null;
      }
      await new Promise((res) => setTimeout(res, 120));
      inFlight = false;
    }
  }

  function setTono(v: number) {
    if (!cal) return;
    send(cal.index, Math.max(base - 90, Math.min(base + 90, Math.round(v))), sat);
  }
  function setSat(v: number) {
    if (!cal) return;
    send(cal.index, tono, Math.max(0.3, Math.min(1.5, Math.round(v * 100) / 100)));
  }
  async function goTo(i: number) {
    if (!cal) return;
    done.add(cal.index);
    const next = ((i % total) + total) % total;
    cal.index = next;
    const p = cal.points[next];
    const r = await attempt(() => commands.calibrationSet(next, p.tono_deg, p.sat));
    if (r.ok) cal = r.value;
    else error = r.error;
  }

  function onKey(e: KeyboardEvent) {
    if (mode !== "hues" || !cal) return;
    const tag = (e.target as HTMLElement).tagName;
    const inRange = (e.target as HTMLInputElement).type === "range";
    if (tag === "INPUT" && !inRange) return;
    if (e.key === "ArrowLeft") setTono(tono - 2);
    else if (e.key === "ArrowRight") setTono(tono + 2);
    else if (e.key === "ArrowUp") setSat(sat + 0.05);
    else if (e.key === "ArrowDown") setSat(sat - 0.05);
    else if (e.key === "Enter" && tag !== "BUTTON") void goTo(cal.index + 1);
    else return;
    e.preventDefault();
  }

  function leave() {
    const to = wizard.returnTo;
    wizard.returnTo = null;
    if (document.fullscreenElement) void document.exitFullscreen();
    go(to === "onboarding" ? "onboarding" : "dashboard");
  }

  async function save(exit: boolean): Promise<void> {
    saving = true;
    const r = await attempt(() => commands.calibrationSave());
    saving = false;
    if (!r.ok) {
      raise(r.error, () => save(exit));
      return;
    }
    await refreshDevices();
    status = t("calibration.saved");
    if (exit) leave();
    else void start();
  }

  async function cancel() {
    await attempt(() => commands.calibrationCancel());
    leave();
  }

  function fullscreen() {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void document.documentElement.requestFullscreen?.();
  }

  const delta = $derived(Math.round(tono - base));
  const shownHue = $derived(((Math.round(tono) % 360) + 360) % 360);
  const nf = $derived(new Intl.NumberFormat(i18n.locale));
</script>

<svelte:window onkeydown={onKey} />

<div class="calibration" style:--screen={mode === "hues" ? hueCss(base) : "#000"}>
  {#if mode === "ab" && device}
    <ABTest {device} onclose={() => (mode = "hues")} />
  {:else}
    <section class="panel" aria-labelledby="cal-title">
      <div class="top">
        <div>
          <span class="eyebrow">{t("calibration.eyebrow", { name: device?.name ?? "" })}</span>
          <h1 id="cal-title" aria-live="polite">{t(hueKey(base))} <small class="tabular">{(cal?.index ?? 0) + 1} / {total || (quick ? 6 : 12)}</small></h1>
        </div>
        <div class="modes" role="group" aria-label={t("calibration.mode")}>
          <button type="button" aria-pressed={!quick} onclick={() => { quick = false; void start(); }}>{t("calibration.full", { n: 12 })}</button>
          <button type="button" aria-pressed={quick} onclick={() => { quick = true; void start(); }}>{t("calibration.quick", { n: 6 })}</button>
        </div>
      </div>
      <p class="help">{t("calibration.help")}</p>
      <p class="keys"><kbd>←</kbd><kbd>→</kbd> {t("calibration.key_hue")} · <kbd>↑</kbd><kbd>↓</kbd> {t("calibration.key_sat")} · <kbd>Enter</kbd> {t("calibration.key_next")}</p>

      {#if error}<ErrorCard {error} retry={start} onresolved={start} />{/if}

      {#if cal && point}
        <div class="slider">
          <label for="cal-hue">{t("calibration.hue")}</label>
          <output for="cal-hue" class="tabular">{nf.format(shownHue)}° ({delta > 0 ? "+" : ""}{delta}°)</output>
          <input id="cal-hue" type="range" min={base - 90} max={base + 90} step="1" value={tono} style:--fill="{((tono - base + 90) / 180) * 100}%" aria-valuetext="{shownHue}°, {delta > 0 ? '+' : ''}{delta}°" oninput={(e) => setTono(Number(e.currentTarget.value))} />
        </div>
        <div class="slider">
          <label for="cal-sat">{t("calibration.sat")}</label>
          <output for="cal-sat" class="tabular">{Math.round(sat * 100)} %</output>
          <input id="cal-sat" type="range" min="0.3" max="1.5" step="0.05" value={sat} style:--fill="{((sat - 0.3) / 1.2) * 100}%" aria-valuetext="{Math.round(sat * 100)} %" oninput={(e) => setSat(Number(e.currentTarget.value))} />
        </div>
        <div class="points" role="group" aria-label={t("calibration.points")}>
          {#each cal.points as p, i (p.screen_hue_deg)}
            <button type="button" style:--c={hueCss(p.screen_hue_deg ?? 0)} class:done={done.has(i)} aria-current={i === cal.index ? "step" : undefined} aria-label={t("calibration.go_to", { name: t(hueKey(p.screen_hue_deg ?? 0)) })} onclick={() => goTo(i)}></button>
          {/each}
        </div>
        <div class="row">
          <button type="button" class="btn" onclick={() => goTo(cal!.index - 1)}><Icon name="back" size={16} />{t("calibration.prev")}</button>
          <button type="button" class="btn primary" onclick={() => goTo(cal!.index + 1)}>{t("calibration.next")}<Icon name="chevron" size={16} /></button>
        </div>
      {:else if !error}
        <p role="status"><span class="spinner" aria-hidden="true"></span> {t("calibration.starting")}</p>
      {/if}

      <div class="row secondary">
        <button type="button" class="btn small" onclick={fullscreen}><Icon name="monitor" size={15} />{t("calibration.fullscreen")}</button>
        <button type="button" class="btn small" onclick={() => (mode = "ab")}><Icon name="eye" size={15} />{t("calibration.ab")}</button>
      </div>
      <div class="row end">
        <button type="button" class="btn small ghost" onclick={cancel}>{t("calibration.cancel")}</button>
        <button type="button" class="btn small" disabled={saving || !cal} onclick={() => save(false)}>{t("calibration.save")}</button>
        <button type="button" class="btn small primary" disabled={saving || !cal} onclick={() => save(true)}>{t("calibration.save_exit")}</button>
      </div>
      <p class="status" role="status">{status}</p>
    </section>
  {/if}
</div>

<style>
  /* El panel es siempre oscuro y translúcido, sobre el color puro de la pantalla (como calibrar.py). */
  .calibration {
    min-height: 100vh;
    background: var(--screen);
    transition: background 150ms;
    display: grid;
    align-items: end;
    justify-items: center;
    padding: 24px 16px;
    color-scheme: dark;
    --text: #f3f4f7;
    --muted: #c4c8d2;
    --faint: #a0a5b3;
    --surface: #1a1c21;
    --surface-2: #2a2d35;
    --surface-3: #3a3e48;
    --line: #4a4f5b;
    --line-strong: #6a6f7c;
    --accent: #f3f4f7;
    --accent-strong: #ffffff;
    --accent-ink: #14161b;
    --accent-soft: rgb(243 244 247 / 0.16);
    --live: #f3f4f7;
    --focus: #8db0ff;
    --danger: #ff8a7d;
    --danger-soft: rgb(255 138 125 / 0.14);
    --bg-deep: #08090b;
    --card-line: transparent;
  }
  .panel {
    width: min(640px, 100%);
    display: grid;
    gap: 0.8rem;
    padding: 1.2rem 1.3rem;
    border-radius: var(--radius);
    background: rgb(18 19 23 / 0.92);
    color: var(--text);
    backdrop-filter: blur(10px);
    box-shadow: 0 30px 60px -30px rgb(0 0 0 / 0.8);
  }
  .top {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
  }
  h1 {
    font-size: 1.8rem;
    display: flex;
    gap: 0.6rem;
    align-items: baseline;
  }
  h1 small {
    font-size: 0.85rem;
    color: var(--muted);
  }
  .modes {
    display: flex;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .modes button {
    border: 0;
    padding: 0.35em 0.8em;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: 700 0.8rem var(--font-body);
    cursor: pointer;
  }
  .modes button[aria-pressed="true"] {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .help,
  .keys {
    font-size: 0.9rem;
    color: var(--muted);
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
  .points {
    display: flex;
    gap: 4px;
  }
  .points button {
    flex: 1;
    height: 12px;
    border: 0;
    border-radius: 6px;
    background: var(--c);
    opacity: 0.35;
    cursor: pointer;
  }
  .points button.done {
    opacity: 1;
  }
  .points button[aria-current="step"] {
    opacity: 1;
    outline: 2px solid #fff;
    outline-offset: 2px;
  }
  .row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .row > .btn:not(.small) {
    flex: 1;
  }
  .secondary {
    padding-top: 0.6rem;
    border-top: 1px solid var(--line);
  }
  .end {
    justify-content: flex-end;
  }
  .status {
    min-height: 1.2em;
    font-size: 0.85rem;
    color: #8fe0a8;
  }
</style>
