<script lang="ts">
  // Un `UserFacingError` con su título, su explicación y sus acciones. Nunca queda sin acción:
  // si el backend no mandó ninguna, se ofrece abrir el diagnóstico.
  import type { FixAction, UserFacingError } from "../bindings";
  import { t } from "../i18n/index.svelte";
  import { app, runFix } from "../app.svelte";
  import { go } from "../router.svelte";
  import Icon from "./Icon.svelte";

  let {
    error,
    retry,
    retryLabel,
    ondismiss,
    onresolved,
    live = true,
  }: {
    error: UserFacingError;
    retry?: () => unknown;
    retryLabel?: string;
    ondismiss?: () => void;
    onresolved?: () => void;
    live?: boolean;
  } = $props();

  let running = $state<number | null>(null);

  function label(a: FixAction): string {
    switch (a.kind) {
      case "open_settings":
        return t(`fix.open_settings.${a.value}`);
      case "renew_key":
        return t("fix.renew_key");
      case "retry":
        return retryLabel ?? t("fix.retry");
      case "remove_from_effect":
        return t("fix.remove_from_effect");
      case "open_url":
        return t("fix.open_url", { host: safeHost(a.value) });
      case "show_help":
        return t("fix.show_help", { topic: t(`help.${a.value}.title`) });
    }
  }

  function safeHost(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  const icons = { open_settings: "sliders", renew_key: "key", retry: "refresh", open_url: "external", show_help: "help", remove_from_effect: "power" } as const;

  async function act(a: FixAction, i: number) {
    running = i;
    const solved = await runFix(a, error, retry);
    running = null;
    if (solved) onresolved?.();
  }

  const renewing = $derived(Boolean(app.busy[`renew:${error.params.device ?? ""}`]));
</script>

<div class="error" role={live ? "alert" : undefined} data-code={error.code}>
  <span class="mark" aria-hidden="true"><Icon name="alert" size={18} /></span>
  <div class="body">
    <p class="title">{t(error.title, error.params)}</p>
    <p class="detail">{t(error.detail, error.params)}</p>
    <div class="actions">
      {#each error.actions as a, i (i)}
        <button
          type="button"
          class="btn small"
          class:primary={i === 0}
          disabled={running !== null || (a.kind === "renew_key" && renewing)}
          onclick={() => act(a, i)}
        >
          {#if running === i || (a.kind === "renew_key" && renewing)}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name={icons[a.kind]} size={16} />{/if}
          {label(a)}
        </button>
      {:else}
        <button type="button" class="btn small primary" onclick={() => go("diagnostics")}>
          <Icon name="pulse" size={16} />{t("fix.open_diagnostics")}
        </button>
      {/each}
    </div>
    <p class="code mono">{error.code}</p>
  </div>
  {#if ondismiss}
    <button type="button" class="close btn ghost small" aria-label={t("common.dismiss")} onclick={ondismiss}>
      <Icon name="close" size={16} />
    </button>
  {/if}
</div>

<style>
  .error {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.75rem;
    padding: 0.95rem 1rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--line));
    background: linear-gradient(160deg, var(--danger-soft), transparent 70%), var(--surface);
    box-shadow: var(--shadow);
  }
  .mark {
    color: var(--danger);
    padding-top: 0.15rem;
  }
  .title {
    font-weight: 700;
  }
  .detail {
    color: var(--muted);
    margin-top: 0.15rem;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    margin-top: 0.7rem;
  }
  .code {
    margin-top: 0.55rem;
    font-size: 0.72rem;
    color: var(--faint);
  }
  .close {
    align-self: start;
    width: 32px;
    padding: 0;
  }
</style>
