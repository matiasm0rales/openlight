<script lang="ts">
  // El aviso de versión nueva: notas, "Actualizar y reiniciar" con el progreso de la descarga y,
  // si algo falla, qué pasó y qué hacer. Se usa en el panel y en Ajustes.
  import { dismissUpdate, installUpdate, restartApp, updates, updating } from "../updater.svelte";
  import { formatBytes } from "../format";
  import { i18n, t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
  import UpdateProblem from "./UpdateProblem.svelte";

  let { level = 2, dismissable = true }: { level?: 2 | 3; dismissable?: boolean } = $props();

  const busy = $derived(updating());
  // Los errores de una búsqueda se muestran junto al botón que la pidió, en Ajustes.
  const problem = $derived(updates.error && updates.error.step !== "check" ? updates.error : null);
  const total = $derived(updates.progress.total);
  const done = $derived(updates.progress.done);
  const amount = $derived.by(() => {
    if (!total) return t("update.progress_unknown", { done: formatBytes(done, i18n.locale) });
    const bytes = t("update.progress", { done: formatBytes(done, i18n.locale), total: formatBytes(total, i18n.locale) });
    return `${bytes} · ${Math.min(100, Math.floor((done / total) * 100))} %`;
  });
</script>

{#if updates.available}
  <section class="update card rise" aria-labelledby="update-title">
    <span class="mark" aria-hidden="true"><Icon name="download" size={22} /></span>
    <div class="body">
      <svelte:element this={`h${level}`} id="update-title" class="title">
        {t(updates.installed ? "update.installed_title" : "update.available_title", { version: updates.available.version })}
      </svelte:element>
      {#if !updates.installed}
        <p class="muted">{t("update.available_lead", { current: updates.available.current })}</p>
        {#if updates.available.notes}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex (región con scroll: tiene que poder enfocarse con el teclado) -->
          <div class="notes" role="region" aria-label={t("update.notes")} tabindex="0">{updates.available.notes}</div>
        {/if}
      {/if}

      {#if busy}
        <div class="working">
          <p class="phase" role="status"><span class="spinner" aria-hidden="true"></span>{t(`update.phase.${updates.phase}`)}</p>
          {#if updates.phase === "downloading"}
            {#if total}
              <progress aria-label={t("update.progress_label")} max={total} value={Math.min(done, total)}></progress>
            {:else}
              <progress aria-label={t("update.progress_label")}></progress>
            {/if}
            <p class="amount muted tabular">{amount}</p>
          {/if}
        </div>
      {:else if problem}
        <UpdateProblem {problem} onretry={installUpdate} />
        {#if dismissable}
          <div class="actions"><button type="button" class="btn ghost small" onclick={dismissUpdate}>{t("update.later")}</button></div>
        {/if}
      {:else}
        <div class="actions">
          {#if updates.installed}
            <button type="button" class="btn primary" onclick={restartApp}><Icon name="power" size={16} />{t("update.action.restart")}</button>
          {:else}
            <button type="button" class="btn primary" onclick={installUpdate} disabled={updates.phase === "checking"}>
              <Icon name="download" size={16} />{t("update.install")}
            </button>
          {/if}
          {#if dismissable}
            <button type="button" class="btn ghost" onclick={dismissUpdate}>{t("update.later")}</button>
          {/if}
        </div>
      {/if}
    </div>
  </section>
{/if}

<style>
  .update {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 1rem;
    padding: 1.1rem 1.3rem;
    border-radius: var(--radius-md);
  }
  .mark {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .body {
    display: grid;
    gap: 0.7rem;
  }
  .title {
    font-size: 1.2rem;
    padding-top: 0.6rem;
  }
  .notes {
    max-height: 8.5rem;
    overflow: auto;
    padding: 0.7rem 0.9rem;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 0.9rem;
    white-space: pre-line;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }
  .working {
    display: grid;
    gap: 0.45rem;
  }
  .phase {
    display: flex;
    gap: 0.55rem;
    align-items: center;
    font-weight: 600;
  }
  .amount {
    font-size: 0.82rem;
  }
  progress {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 10px;
    border: 0;
    border-radius: 999px;
    overflow: hidden;
    background: var(--surface-3);
  }
  progress::-webkit-progress-bar {
    background: var(--surface-3);
  }
  progress::-webkit-progress-value {
    border-radius: 999px;
    background: var(--live);
    transition: width var(--dur) var(--ease);
  }
  progress::-moz-progress-bar {
    border-radius: 999px;
    background: var(--live);
  }
  progress:indeterminate {
    animation: pulse 1.2s infinite;
  }
</style>
