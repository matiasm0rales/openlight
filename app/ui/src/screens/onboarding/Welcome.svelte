<script lang="ts">
  import type { ImportReport, OnboardingStep, UserFacingError } from "../../lib/bindings";
  import { attempt, commands } from "../../lib/ipc";
  import { refreshDevices, refreshSettings } from "../../lib/app.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import ErrorCard from "../../lib/components/ErrorCard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable(), jump }: { valid: boolean; completed: boolean; jump: (s: OnboardingStep, done?: OnboardingStep[]) => void } = $props();
  $effect(() => {
    valid = true;
  });

  let importing = $state(false);
  let showImport = $state(false);
  let folder = $state("");
  let error = $state<UserFacingError | null>(null);
  let report = $state<ImportReport | null>(null);

  async function runImport() {
    importing = true;
    error = null;
    const r = await attempt(() => commands.importLegacy(folder.trim()));
    importing = false;
    if (!r.ok) {
      error = r.error;
      return;
    }
    report = r.value;
    await Promise.all([refreshDevices(), refreshSettings()]);
  }

  const needs = ["bulbs", "wifi", "time"] as const;
  const icons = { bulbs: "bulb", wifi: "wifi", time: "target" } as const;
</script>

<div class="welcome">
  <ul class="needs">
    {#each needs as n, i (n)}
      <li class="card rise" style:animation-delay="{120 + i * 70}ms">
        <span class="ico"><Icon name={icons[n]} size={22} /></span>
        <div><p class="h">{t(`onboarding.welcome.need.${n}.title`)}</p><p class="muted">{t(`onboarding.welcome.need.${n}.text`)}</p></div>
      </li>
    {/each}
  </ul>

  <div class="why">
    <h2>{t("onboarding.welcome.why_title")}</h2>
    <p class="muted">{t("onboarding.welcome.why_text")}</p>
  </div>

  <div class="import card">
    <div class="row">
      <Icon name="folder" size={22} />
      <div>
        <p class="h">{t("onboarding.welcome.import_title")}</p>
        <p class="muted">{t("onboarding.welcome.import_text")}</p>
      </div>
      <button type="button" class="btn" aria-expanded={showImport} aria-controls="import-form" onclick={() => (showImport = !showImport)}>
        {t("onboarding.welcome.import_cta")}
      </button>
    </div>
    {#if showImport}
      <form id="import-form" class="form" onsubmit={(e) => { e.preventDefault(); void runImport(); }}>
        <div class="field">
          <label for="legacy-folder">{t("onboarding.welcome.import_folder")}</label>
          <input id="legacy-folder" type="text" bind:value={folder} placeholder="~/Documents/Open RGB" autocomplete="off" spellcheck="false" aria-describedby="legacy-hint" />
          <p id="legacy-hint" class="muted small">{t("onboarding.welcome.import_hint")}</p>
        </div>
        <button type="submit" class="btn primary" disabled={!folder.trim() || importing}>
          {#if importing}<span class="spinner" aria-hidden="true"></span>{/if}{t("onboarding.welcome.import_run")}
        </button>
      </form>
      {#if error}<ErrorCard {error} retry={runImport} />{/if}
      {#if report}
        <div class="report" role="status">
          <p class="h"><Icon name="check" size={18} /> {t("onboarding.welcome.import_ok", { devices: report.devices.length, calibrations: report.calibrations })}</p>
          {#each report.warnings as w (w)}<p class="muted">{t(w)}</p>{/each}
          <button type="button" class="btn primary" onclick={() => jump("zones", ["welcome", "permissions", "tuya_account", "cloud_project", "link_app", "credentials", "choose_devices", "test_devices"])}>
            {t("onboarding.welcome.import_continue")}<Icon name="chevron" size={16} />
          </button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .welcome {
    display: grid;
    gap: 1.8rem;
  }
  .needs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 0.8rem;
  }
  .needs li {
    display: flex;
    gap: 0.8rem;
    padding: 1rem;
  }
  .ico {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .h {
    font-weight: 700;
  }
  .why {
    display: grid;
    gap: 0.5rem;
    max-width: 64ch;
    padding-left: 1rem;
    border-left: 2px solid var(--accent);
  }
  .why h2 {
    font-size: 1.2rem;
  }
  .import {
    display: grid;
    gap: 1rem;
    padding: 1rem 1.1rem;
  }
  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.9rem;
    align-items: center;
  }
  .form {
    display: flex;
    gap: 0.8rem;
    align-items: start;
    flex-wrap: wrap;
  }
  .form .field {
    flex: 1 1 320px;
  }
  .form .btn {
    margin-top: 1.75rem;
  }
  .small {
    font-size: 0.84rem;
  }
  .report {
    display: grid;
    gap: 0.6rem;
    justify-items: start;
    padding: 1rem;
    border-radius: var(--radius-sm);
    background: var(--ok-soft);
  }
  .report .h {
    display: flex;
    gap: 0.4rem;
    color: var(--ok);
  }
</style>
