<script lang="ts">
  // A. Asistente inicial: 11 pasos con progreso que persiste (`onboarding_save`), se puede volver
  // atrás, pausar y retomar, y cada paso valida antes de habilitar "Siguiente".
  import type { Component } from "svelte";
  import type { OnboardingStep } from "../lib/bindings";
  import { app, raise } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { ONBOARDING_STEPS } from "../lib/wizard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import LanguageSwitch from "../lib/components/LanguageSwitch.svelte";
  import Welcome from "./onboarding/Welcome.svelte";
  import Permissions from "./onboarding/Permissions.svelte";
  import TuyaAccount from "./onboarding/TuyaAccount.svelte";
  import CloudProject from "./onboarding/CloudProject.svelte";
  import LinkApp from "./onboarding/LinkApp.svelte";
  import Credentials from "./onboarding/Credentials.svelte";
  import ChooseDevices from "./onboarding/ChooseDevices.svelte";
  import TestDevices from "./onboarding/TestDevices.svelte";
  import Zones from "./onboarding/Zones.svelte";
  import Calibrate from "./onboarding/Calibrate.svelte";
  import Done from "./onboarding/Done.svelte";

  type StepApi = { next?: () => Promise<boolean> };
  type StepProps = { valid: boolean; completed: boolean; jump: (step: OnboardingStep, done?: OnboardingStep[]) => void };

  const VIEWS: Record<OnboardingStep, Component<StepProps, StepApi, "valid">> = {
    welcome: Welcome,
    permissions: Permissions,
    tuya_account: TuyaAccount,
    cloud_project: CloudProject,
    link_app: LinkApp,
    credentials: Credentials,
    choose_devices: ChooseDevices,
    test_devices: TestDevices,
    zones: Zones,
    calibrate: Calibrate,
    done: Done,
  };

  let step = $state<OnboardingStep>(app.onboarding?.step ?? "welcome");
  let completed = $state<OnboardingStep[]>([...(app.onboarding?.completed ?? [])]);
  let valid = $state(false);
  let saving = $state(false);
  let view: StepApi | undefined = $state();
  let heading: HTMLHeadingElement | undefined = $state();

  const index = $derived(ONBOARDING_STEPS.indexOf(step));
  const View = $derived(VIEWS[step]);
  const isLast = $derived(index === ONBOARDING_STEPS.length - 1);

  async function save(next: OnboardingStep, done: OnboardingStep[]) {
    const r = await attempt(() => commands.onboardingSave(next, done));
    if (r.ok) app.onboarding = r.value;
    else raise(r.error);
  }

  async function moveTo(next: OnboardingStep, done = completed) {
    completed = [...new Set(done)];
    step = next;
    valid = false;
    await save(next, completed);
    queueMicrotask(() => heading?.focus());
  }

  async function nextStep() {
    if (!valid || saving) return;
    saving = true;
    const ok = (await view?.next?.()) ?? true;
    saving = false;
    if (!ok) return;
    if (isLast) return;
    await moveTo(ONBOARDING_STEPS[index + 1], [...completed, step]);
  }

  function back() {
    if (index > 0) void moveTo(ONBOARDING_STEPS[index - 1]);
  }

  async function pause() {
    await save(step, completed);
    go("dashboard");
  }

  function jump(target: OnboardingStep, done: OnboardingStep[] = []) {
    void moveTo(target, [...completed, ...done]);
  }

  function reachable(s: OnboardingStep) {
    return completed.includes(s) || s === step || completed.includes(ONBOARDING_STEPS[ONBOARDING_STEPS.indexOf(s) - 1]);
  }
</script>

<div class="wizard">
  <aside class="progress" aria-label={t("onboarding.progress_label")}>
    <div class="brand"><span class="eyebrow">OpenLight</span><span class="count tabular">{index + 1}/{ONBOARDING_STEPS.length}</span></div>
    <div
      class="bar"
      role="progressbar"
      aria-label={t("onboarding.progress_label")}
      aria-valuemin={1}
      aria-valuemax={ONBOARDING_STEPS.length}
      aria-valuenow={index + 1}
      aria-valuetext={t("onboarding.progress_value", { n: index + 1, total: ONBOARDING_STEPS.length, name: t(`onboarding.${step}.short`) })}
    >
      <i style:height="{(index / (ONBOARDING_STEPS.length - 1)) * 100}%"></i>
    </div>
    <ol>
      {#each ONBOARDING_STEPS as s, i (s)}
        <li class:done={completed.includes(s)} class:current={s === step}>
          <button type="button" disabled={!reachable(s) || s === step} aria-current={s === step ? "step" : undefined} onclick={() => moveTo(s)}>
            <span class="dot" aria-hidden="true">{#if completed.includes(s) && s !== step}<Icon name="check" size={12} />{/if}</span>
            <span class="name"><span class="sr-only">{t("onboarding.step_n", { n: i + 1 })}</span>{t(`onboarding.${s}.short`)}</span>
          </button>
        </li>
      {/each}
    </ol>
    <div class="foot">
      <LanguageSwitch />
      <button type="button" class="btn ghost small pause" onclick={pause}><Icon name="back" size={16} />{t("onboarding.pause")}</button>
    </div>
  </aside>

  <section class="content" aria-labelledby="step-title">
    {#key step}
      <header class="rise">
        <span class="eyebrow">{t("onboarding.step_n", { n: index + 1 })}</span>
        <h1 id="step-title" tabindex="-1" bind:this={heading}>{t(`onboarding.${step}.title`)}</h1>
        <p class="lead">{t(`onboarding.${step}.lead`)}</p>
      </header>
      <div class="body rise" style:animation-delay="80ms">
        <View bind:this={view} bind:valid completed={completed.includes(step)} {jump} />
      </div>
    {/key}

    <footer class="nav">
      <button type="button" class="btn" onclick={back} disabled={index === 0}><Icon name="back" size={16} />{t("onboarding.back")}</button>
      {#if !isLast}
        <span class="hint muted" aria-live="polite">{valid ? "" : t(`onboarding.${step}.needs`)}</span>
        <button type="button" class="btn primary" onclick={nextStep} aria-disabled={!valid || saving} disabled={saving} data-testid="next">
          {#if saving}<span class="spinner" aria-hidden="true"></span>{/if}
          {t("onboarding.next")}<Icon name="chevron" size={16} />
        </button>
      {/if}
    </footer>
  </section>
</div>

<style>
  .wizard {
    display: grid;
    grid-template-columns: 270px 1fr;
    min-height: 100vh;
  }
  .progress {
    position: sticky;
    top: 0;
    height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
    grid-template-columns: 14px 1fr;
    column-gap: 1rem;
    padding: 2rem 1.4rem 1.4rem;
    border-right: 1px solid var(--line);
    background: var(--bg-deep);
  }
  .brand {
    grid-column: 1 / -1;
    display: flex;
    justify-content: space-between;
    margin-bottom: 1.6rem;
  }
  .count {
    font-size: 0.8rem;
    color: var(--accent);
  }
  /* El filamento: una línea que se enciende a medida que se avanza. */
  .bar {
    grid-row: 2;
    position: relative;
    width: 2px;
    margin: 12px 0 0 6px;
    height: calc(100% - 46px);
    max-height: 520px;
    background: var(--line);
    border-radius: 2px;
  }
  .bar i {
    position: absolute;
    left: 0;
    top: 0;
    width: 2px;
    background: var(--accent);
    box-shadow: 0 0 12px var(--live);
    transition: height 500ms var(--ease);
  }
  ol {
    grid-row: 2;
    grid-column: 1 / -1;
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    align-content: start;
    gap: 0.15rem;
    max-height: 560px;
  }
  ol button {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    width: 100%;
    padding: 0.4rem 0.2rem;
    border: 0;
    background: none;
    color: var(--faint);
    font: 0.93rem var(--font-body);
    text-align: left;
    cursor: pointer;
  }
  ol button:disabled {
    cursor: default;
  }
  .dot {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    flex: none;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    background: var(--bg-deep);
    color: var(--accent-ink);
  }
  li.done button {
    color: var(--muted);
  }
  li.done .dot {
    border-color: var(--accent);
    background: var(--accent);
  }
  li.current button {
    color: var(--text);
    font-weight: 700;
  }
  li.current .dot {
    border-color: var(--accent);
    box-shadow: 0 0 0 4px var(--accent-soft);
  }
  .foot {
    grid-column: 1 / -1;
    display: grid;
    gap: 0.9rem;
    justify-items: start;
  }
  .foot > :global(.lang) {
    justify-self: stretch;
  }
  .content {
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 1.6rem;
    max-width: 980px;
    width: 100%;
    padding: clamp(1.5rem, 4vw, 3.2rem) clamp(1.2rem, 4vw, 3.6rem) 1.4rem;
  }
  header {
    display: grid;
    gap: 0.7rem;
  }
  h1 {
    outline: none;
    max-width: 22ch;
  }
  .lead {
    font-size: 1.08rem;
    color: var(--muted);
    max-width: 62ch;
  }
  .nav {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem 0;
    border-top: 1px solid var(--line);
    background: linear-gradient(transparent, var(--bg) 30%);
  }
  .hint {
    margin-left: auto;
    font-size: 0.88rem;
    text-align: right;
  }
  .nav .primary {
    min-width: 150px;
  }
  .nav .primary[aria-disabled="true"] {
    opacity: 0.5;
    cursor: not-allowed;
  }
  @media (max-width: 820px) {
    .wizard {
      grid-template-columns: 1fr;
    }
    .progress {
      position: static;
      height: auto;
    }
    ol {
      display: flex;
      flex-wrap: wrap;
    }
    .name {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
    }
    .bar {
      display: none;
    }
  }
</style>
