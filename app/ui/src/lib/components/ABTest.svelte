<script lang="ts">
  // Prueba A/B guiada ("¿cuál se parece más?") para elegir `send_channel` y `color_mode` de un foco.
  // Las preguntas y sus opciones salen del catálogo (ajustes de tipo opción); `send_channel` solo
  // aparece si el foco tiene DP de control. Cada variante se aplica al foco mientras corre un
  // video de prueba en esta ventana, y la elegida queda guardada en los ajustes del foco.
  import type { ConfiguredDevice_Serialize, SettingDef, SettingId } from "../bindings";
  import { app, raise, refreshSettings, toggleSession } from "../app.svelte";
  import { attempt, commands } from "../ipc";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let { device, onclose }: { device: ConfiguredDevice_Serialize; onclose: () => void } = $props();

  const QUESTIONS: SettingId[] = ["send_channel", "color_mode"];
  const SECONDS = 6;

  const defs = $derived(
    (app.settings?.catalog ?? []).filter(
      (d): d is SettingDef & { kind: { kind: "choice"; options: string[] } } =>
        QUESTIONS.includes(d.id) && d.kind.kind === "choice" && (d.id !== "send_channel" || Boolean(device.capabilities.dp_control)),
    ),
  );

  let q = $state(0);
  let phase = $state<"intro" | "a" | "b" | "choose" | "summary">("intro");
  let left = $state(SECONDS);
  let chosen = $state<Record<string, string>>({});
  let timer: ReturnType<typeof setInterval> | undefined;

  const def = $derived(defs[q]);
  // A = el valor actual (o el default), B = la primera alternativa.
  const pair = $derived.by<[string, string]>(() => {
    if (!def) return ["", ""];
    const current = String(device.overrides[def.id] ?? app.settings?.effective[def.id] ?? def.default);
    const other = def.kind.options.find((o) => o !== current) ?? def.kind.options[0];
    return [current, other];
  });

  $effect(() => {
    if (!app.settings) void refreshSettings();
    return () => clearInterval(timer);
  });

  async function apply(value: string) {
    if (!def) return;
    const r = await attempt(() => commands.settingsSet({ kind: "device", id: device.id }, def.id, value));
    if (r.ok) app.settings = r.value;
    else raise(r.error);
  }

  async function play(which: "a" | "b") {
    phase = which;
    left = SECONDS;
    await apply(which === "a" ? pair[0] : pair[1]);
    clearInterval(timer);
    timer = setInterval(() => {
      left -= 1;
      if (left > 0) return;
      clearInterval(timer);
      if (which === "a") void play("b");
      else phase = "choose";
    }, 1000);
  }

  async function pick(value: string) {
    chosen[def!.id] = value;
    await apply(value);
    if (q + 1 < defs.length) {
      q += 1;
      phase = "intro";
    } else phase = "summary";
  }

  const label = (id: string, o: string) => t(`settings.${id}.option.${o}`);
</script>

<div class="ab">
  <div class="video" aria-hidden="true" class:running={phase === "a" || phase === "b"}></div>
  <section class="panel" aria-labelledby="ab-title" aria-live="polite">
    <span class="eyebrow">{t("ab.eyebrow", { name: device.name })}</span>
    {#if phase === "summary" || !def}
      <h1 id="ab-title">{t("ab.done_title")}</h1>
      <ul class="summary">
        {#each Object.entries(chosen) as [id, v] (id)}<li><span class="muted">{t(`settings.${id}.title`)}</span> <strong>{label(id, v)}</strong></li>{/each}
      </ul>
      {#if !defs.length}<p class="muted">{t("ab.nothing")}</p>{/if}
      <button type="button" class="btn primary" onclick={onclose}>{t("ab.back")}</button>
    {:else}
      <h1 id="ab-title">{t("ab.question", { n: q + 1, total: defs.length, setting: t(def.title_key) })}</h1>
      <p class="muted">{t(def.help_key)}</p>
      {#if app.session !== "active"}
        <div class="need">
          <p>{t("ab.needs_session")}</p>
          <button type="button" class="btn primary small" onclick={toggleSession}><Icon name="power" size={15} />{t("session.start")}</button>
        </div>
      {/if}
      <div class="variants">
        {#each ["a", "b"] as const as v, i (v)}
          <div class="variant" class:on={phase === v}>
            <span class="letter">{v.toUpperCase()}</span>
            <span>{label(def.id, pair[i])}</span>
            {#if phase === v}<span class="count tabular">{t("ab.seconds", { n: left })}</span>{/if}
          </div>
        {/each}
      </div>
      {#if phase === "intro"}
        <p>{t("ab.intro")}</p>
        <button type="button" class="btn primary" disabled={app.session !== "active"} onclick={() => play("a")}><Icon name="play" size={16} />{t("ab.start")}</button>
      {:else if phase === "choose"}
        <p class="q">{t("ab.which")}</p>
        <div class="row">
          <button type="button" class="btn primary" onclick={() => pick(pair[0])}>{t("ab.pick", { v: "A" })}</button>
          <button type="button" class="btn primary" onclick={() => pick(pair[1])}>{t("ab.pick", { v: "B" })}</button>
          <button type="button" class="btn" onclick={() => pick(pair[0])}>{t("ab.same")}</button>
          <button type="button" class="btn ghost" onclick={() => play("a")}><Icon name="refresh" size={15} />{t("ab.repeat")}</button>
        </div>
      {/if}
      <button type="button" class="btn ghost small close" onclick={onclose}>{t("ab.back")}</button>
    {/if}
  </section>
</div>

<style>
  .ab {
    position: relative;
    width: 100%;
    min-height: calc(100vh - 48px);
    display: grid;
    align-items: end;
    justify-items: center;
  }
  /* "Video" de prueba: cortes de escena y transiciones para que se note la diferencia. */
  .video {
    position: fixed;
    inset: 0;
    background: linear-gradient(120deg, #ff3b2f, #ffb000 30%, #2bd96b 55%, #2a6bff 80%, #c03bff);
    background-size: 400% 400%;
  }
  .video.running {
    animation: scenes 9s steps(1, end) infinite, drift 3s ease-in-out infinite alternate;
  }
  @keyframes scenes {
    0% {
      background-position: 0% 50%;
    }
    25% {
      background-position: 100% 50%;
    }
    50% {
      background-position: 50% 0%;
    }
    75% {
      background-position: 30% 100%;
    }
  }
  @keyframes drift {
    to {
      filter: hue-rotate(40deg) brightness(0.8);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    /* Sin animación: la persona puede igual comparar con la pantalla fija. */
    .video.running {
      animation: none;
    }
  }
  .panel {
    position: relative;
    width: min(600px, 100%);
    display: grid;
    gap: 0.8rem;
    padding: 1.2rem 1.3rem;
    border-radius: 16px;
    background: rgb(18 16 13 / 0.92);
    color: var(--text);
  }
  h1 {
    font-size: 1.5rem;
  }
  .variants {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .variant {
    display: grid;
    gap: 0.2rem;
    padding: 0.8rem;
    border-radius: 12px;
    border: 1px solid var(--line);
  }
  .variant.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .letter {
    font: 600 1.6rem var(--font-display);
    color: var(--accent);
  }
  .count {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .need {
    display: flex;
    gap: 0.8rem;
    align-items: center;
    padding: 0.7rem 0.9rem;
    border-radius: 10px;
    background: var(--accent-soft);
  }
  .q {
    font-weight: 700;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .summary {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.4rem;
  }
  .close {
    justify-self: end;
  }
</style>
