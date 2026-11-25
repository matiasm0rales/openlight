<script lang="ts">
  // Un error del updater con lo que se puede hacer. Mismo aspecto que `ErrorCard`, que es para los
  // errores del backend (`UserFacingError`).
  import { restartApp, type UpdateProblem } from "../updater.svelte";
  import { RELEASES_URL, updateFixes, type UpdateFix } from "../updates";
  import { openExternal } from "../ipc";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let { problem, onretry }: { problem: UpdateProblem; onretry: () => unknown } = $props();

  const icons = { retry: "refresh", web: "external", restart: "power" } as const;
  const actions: Record<UpdateFix, () => unknown> = {
    retry: () => onretry(),
    web: () => openExternal(RELEASES_URL),
    restart: () => restartApp(),
  };
</script>

<div class="problem" role="alert" data-update-error={problem.kind}>
  <span class="mark" aria-hidden="true"><Icon name="alert" size={18} /></span>
  <div>
    <p class="title">{t(`update.error.${problem.kind}.title`)}</p>
    <p class="detail">{t(`update.error.${problem.kind}.detail`)}</p>
    <div class="actions">
      {#each updateFixes(problem.kind) as fix, i (fix)}
        <button type="button" class="btn small" class:primary={i === 0} onclick={actions[fix]}>
          <Icon name={icons[fix]} size={16} />{t(`update.action.${fix}`)}
        </button>
      {/each}
    </div>
    <p class="cause mono">{problem.cause}</p>
  </div>
</div>

<style>
  .problem {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.75rem;
    padding: 0.95rem 1rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--line));
    background: linear-gradient(160deg, var(--danger-soft), transparent 70%), var(--surface);
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
  .cause {
    margin-top: 0.55rem;
    font-size: 0.72rem;
    color: var(--faint);
    overflow-wrap: anywhere;
  }
</style>
