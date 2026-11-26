<script lang="ts">
  // Paso 8: conexión local y secuencia rojo/verde/azul. "¿Lo viste?" Sí → sigue; No → diagnóstico guiado.
  import type { TestPattern, UserFacingError } from "../../lib/bindings";
  import { attempt, commands } from "../../lib/ipc";
  import { app, refreshDevices } from "../../lib/app.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import { everyWhileVisible } from "../../lib/visibility";
  import ErrorCard from "../../lib/components/ErrorCard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();

  let running = $state<Record<string, TestPattern | null>>({});
  let phase = $state<Record<string, number>>({});
  let errors = $state<Record<string, UserFacingError | null>>({});
  let asked = $state<Record<string, boolean>>({});

  $effect(() => {
    valid = app.devices.length > 0 && app.devices.every((d) => wizard.tested[d.id] === "yes");
  });

  async function run(id: string, pattern: TestPattern) {
    running[id] = pattern;
    errors[id] = null;
    phase[id] = 0;
    const stopTick = everyWhileVisible(() => (phase[id] = ((phase[id] ?? 0) + 1) % 3), 1500);
    const r = await attempt(() => commands.deviceTest(id, pattern));
    stopTick();
    running[id] = null;
    if (!r.ok) {
      errors[id] = r.error;
      return;
    }
    asked[id] = true;
    delete wizard.tested[id];
  }

  async function remove(id: string) {
    const r = await attempt(() => commands.deviceRemove(id));
    if (!r.ok) errors[id] = r.error;
    await refreshDevices();
  }

  const SEQ = ["#ff2a1a", "#19e04b", "#2f6bff"];
  const CHECKS = ["wifi", "smart_life", "switch", "key"] as const;
</script>

<ul class="devices">
  {#each app.devices as d (d.id)}
    {@const answer = wizard.tested[d.id]}
    <li class="card" data-device={d.id}>
      <div class="top">
        <div class="seq" aria-hidden="true">
          {#each SEQ as c, i (c)}<i style:--c={c} class:lit={running[d.id] === "red_green_blue" && phase[d.id] === i} class:blink={running[d.id] === "blink"}></i>{/each}
        </div>
        <div class="info">
          <p class="name">{d.name}</p>
          <p class="muted mono small">{d.ip ?? t("devices.auto_ip")} · v{d.version}</p>
        </div>
        <div class="state" aria-live="polite">
          {#if running[d.id]}
            <span class="chip accent"><span class="spinner" aria-hidden="true"></span>{t("onboarding.test_devices.running")}</span>
          {:else if answer === "yes"}
            <span class="chip ok"><Icon name="check" size={14} />{t("onboarding.test_devices.works")}</span>
          {/if}
        </div>
      </div>

      {#if errors[d.id]}
        <ErrorCard error={errors[d.id]!} retry={() => run(d.id, "red_green_blue")} onresolved={() => run(d.id, "red_green_blue")} />
      {/if}

      {#if asked[d.id] && !answer}
        <div class="ask" role="group" aria-labelledby="ask-{d.id}">
          <p id="ask-{d.id}" class="q">{t("onboarding.test_devices.question", { name: d.name })}</p>
          <button type="button" class="btn primary" onclick={() => (wizard.tested[d.id] = "yes")}><Icon name="check" size={16} />{t("onboarding.test_devices.yes")}</button>
          <button type="button" class="btn" onclick={() => (wizard.tested[d.id] = "no")}>{t("onboarding.test_devices.no")}</button>
        </div>
      {/if}

      {#if answer === "no"}
        <div class="trouble">
          <p class="q">{t("onboarding.test_devices.trouble_title")}</p>
          <ol>{#each CHECKS as c (c)}<li>{t(`onboarding.test_devices.check.${c}`)}</li>{/each}</ol>
          <div class="row">
            <button type="button" class="btn primary small" onclick={() => run(d.id, "red_green_blue")}><Icon name="refresh" size={14} />{t("onboarding.test_devices.again")}</button>
            <button type="button" class="btn small" onclick={() => run(d.id, "blink")}><Icon name="bulb" size={14} />{t("onboarding.test_devices.blink")}</button>
            <button type="button" class="btn small" onclick={() => (app.help = "device_troubleshooting")}><Icon name="help" size={14} />{t("onboarding.how")}</button>
            <button type="button" class="btn ghost small danger" onclick={() => remove(d.id)}><Icon name="trash" size={14} />{t("onboarding.test_devices.remove")}</button>
          </div>
        </div>
      {/if}

      {#if !asked[d.id] && !running[d.id] && answer !== "yes"}
        <button type="button" class="btn primary go" onclick={() => run(d.id, "red_green_blue")}><Icon name="play" size={16} />{t("onboarding.test_devices.run")}</button>
      {/if}
    </li>
  {:else}
    <li class="card empty"><p>{t("onboarding.test_devices.empty")}</p></li>
  {/each}
</ul>

<style>
  .devices {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.9rem;
  }
  li {
    display: grid;
    gap: 0.9rem;
    padding: 1.1rem;
  }
  .top {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 1rem;
    align-items: center;
  }
  .seq {
    display: flex;
    gap: 6px;
  }
  .seq i {
    width: 18px;
    height: 40px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--c) 18%, var(--surface-2));
    transition: background 200ms, box-shadow 200ms;
  }
  .seq i.lit {
    background: var(--c);
    box-shadow: 0 0 22px var(--c);
  }
  .seq i.blink {
    animation: pulse 500ms infinite;
  }
  .name {
    font-weight: 700;
    font-size: 1.05rem;
  }
  .small {
    font-size: 0.82rem;
  }
  .ask,
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }
  .q {
    font-weight: 700;
    margin-right: 0.5rem;
  }
  .trouble {
    display: grid;
    gap: 0.6rem;
    padding: 1rem;
    border-radius: var(--radius-sm);
    background: var(--warn-soft);
  }
  .trouble ol {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.3rem;
  }
  .go {
    justify-self: start;
  }
</style>
