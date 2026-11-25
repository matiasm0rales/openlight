<script lang="ts">
  // Ajustes → Actualizaciones: versión instalada, búsqueda a mano y búsqueda automática. No es un
  // ajuste del catálogo (`settings_get`): es de la app, no de los focos, y se guarda local.
  import { checkForUpdates, setAutoCheck, updates, updating } from "../updater.svelte";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";
  import UpdateCard from "./UpdateCard.svelte";
  import UpdateProblem from "./UpdateProblem.svelte";

  const checking = $derived(updates.phase === "checking");
  const problem = $derived(updates.error?.step === "check" ? updates.error : null);
</script>

<section class="updates card" aria-labelledby="updates-title">
  <header>
    <div>
      <h2 id="updates-title">{t("update.section_title")}</h2>
      <p class="muted">{t("update.current_version", { version: updates.version ?? "…" })}</p>
    </div>
    <button type="button" class="btn" onclick={() => checkForUpdates(true)} disabled={checking || updating()}>
      {#if checking}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="refresh" size={16} />{/if}{t("update.check")}
    </button>
  </header>

  <p class="status" role="status">
    {#if checking}
      {t("update.checking")}
    {:else if updates.upToDate && !updates.available}
      <Icon name="check" size={16} />{t("update.up_to_date")}
    {/if}
  </p>

  {#if problem}<UpdateProblem {problem} onretry={() => checkForUpdates(true)} />{/if}
  <UpdateCard level={3} dismissable={false} />

  <div class="auto">
    <Switch checked={updates.auto} label={t("update.auto")} onchange={setAutoCheck} />
    <div>
      <p class="h" aria-hidden="true">{t("update.auto")}</p>
      <p class="muted small">{t("update.auto_help")}</p>
    </div>
  </div>
</section>

<style>
  .updates {
    display: grid;
    gap: 0.9rem;
    padding: 1.1rem 1.3rem;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.8rem;
  }
  h2 {
    font-size: 1.25rem;
  }
  .status {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-weight: 600;
  }
  .status:empty {
    position: absolute;
  }
  /* Dentro de esta tarjeta el aviso no lleva otra tarjeta encima: se separa con una línea. */
  .updates :global(.update) {
    padding: 0.9rem 0 0;
    border: 0;
    border-top: 1px solid var(--line);
    border-radius: 0;
    box-shadow: none;
  }
  .auto {
    display: flex;
    gap: 0.9rem;
    align-items: center;
    padding-top: 0.9rem;
    border-top: 1px solid var(--line);
  }
  .h {
    font-weight: 700;
  }
  .small {
    font-size: 0.84rem;
  }
</style>
