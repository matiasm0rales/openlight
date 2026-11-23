<script lang="ts">
  // Paso 10: recomendado pero opcional. Abre la calibración (C) y vuelve acá.
  import { app } from "../../lib/app.svelte";
  import { go } from "../../lib/router.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();
  $effect(() => {
    valid = true;
  });

  function calibrate(id: string, quick: boolean) {
    wizard.returnTo = "onboarding";
    go("calibrate", `${id}${quick ? "~quick" : ""}`);
  }
</script>

<div class="cal">
  <p class="muted">{t("onboarding.calibrate.why")}</p>
  <ul>
    {#each app.devices as d (d.id)}
      <li class="card">
        <span class="wheel" aria-hidden="true"></span>
        <div class="info">
          <p class="name">{d.name}</p>
          <p class="muted small">{d.calibrated ? t("devices.calibrated") : t("devices.not_calibrated")}</p>
        </div>
        <button type="button" class="btn primary" onclick={() => calibrate(d.id, false)}><Icon name="palette" size={16} />{t("onboarding.calibrate.full")}</button>
        <button type="button" class="btn" onclick={() => calibrate(d.id, true)}>{t("onboarding.calibrate.quick")}</button>
      </li>
    {/each}
  </ul>
  <p class="muted small">{t("onboarding.calibrate.skip_hint")}</p>
</div>

<style>
  .cal {
    display: grid;
    gap: 1rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.7rem;
  }
  li {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 0.9rem;
    align-items: center;
    padding: 1rem;
  }
  .wheel {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: conic-gradient(#f33, #f93, #ff3, #9f3, #3f3, #3f9, #3ff, #39f, #33f, #93f, #f3f, #f39, #f33);
    mask: radial-gradient(circle, transparent 42%, #000 44%);
  }
  .name {
    font-weight: 700;
  }
  .small {
    font-size: 0.86rem;
  }
</style>
