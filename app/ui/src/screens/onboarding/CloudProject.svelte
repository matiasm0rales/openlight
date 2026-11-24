<script lang="ts">
  import { untrack } from "svelte";
  // Paso 4: proyecto Cloud + "¿dónde creaste tu cuenta de Smart Life?" → Data Center.
  import type { Region } from "../../lib/bindings";
  import { guide } from "../../lib/content";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import GuideStep from "../../lib/components/GuideStep.svelte";

  let { valid = $bindable(), completed }: { valid: boolean; completed: boolean } = $props();
  let confirmed = $state(wizard.confirmed.cloud_project ?? untrack(() => completed));
  let where = $state<number | null>(wizard.region ? guide().data_centers.findIndex((d) => d.region === wizard.region) : null);
  const region = $derived<Region | null>(where === null || where < 0 ? null : guide().data_centers[where].region);

  $effect(() => {
    wizard.confirmed.cloud_project = confirmed;
    if (region) wizard.region = region;
    valid = confirmed && region !== null;
  });
</script>

<GuideStep section="cloud_project" help="cloud_project" confirmLabel={t("onboarding.cloud_project.confirm")} bind:confirmed>
  <fieldset class="dc">
    <legend>{t("onboarding.cloud_project.where")}</legend>
    <div class="options">
      {#each guide().data_centers as d, i (d.region)}
        <label class:on={where === i}>
          <input type="radio" name="where" value={i} bind:group={where} />
          <span>{d.where}</span>
        </label>
      {/each}
    </div>
    {#if region}
      <p class="answer" role="status">
        {t("onboarding.cloud_project.choose")} <strong>{t(`region.${region}`)}</strong>
      </p>
    {/if}
  </fieldset>
</GuideStep>

<style>
  .dc {
    margin: 0;
    padding: 1rem;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface);
    display: grid;
    gap: 0.7rem;
  }
  legend {
    padding: 0 0.4rem;
    font-weight: 700;
  }
  .options {
    display: grid;
    gap: 0.3rem;
  }
  label {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    padding: 0.4rem 0.5rem;
    border-radius: 8px;
    cursor: pointer;
  }
  label.on {
    background: var(--accent-soft);
  }
  .answer {
    padding: 0.7rem 0.9rem;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .answer strong {
    color: var(--accent);
    font-family: var(--font-mono);
  }
</style>
