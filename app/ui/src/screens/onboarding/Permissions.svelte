<script lang="ts">
  // Paso 2: estado en vivo de los permisos del SO, con re-chequeo automático.
  import type { PermissionStatus_Serialize, UserFacingError } from "../../lib/bindings";
  import { attempt, commands } from "../../lib/ipc";
  import { app, repairScreenPermission } from "../../lib/app.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { everyWhileVisible } from "../../lib/visibility";
  import ErrorCard from "../../lib/components/ErrorCard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();
  let perms = $state<PermissionStatus_Serialize[] | null>(null);
  let error = $state<UserFacingError | null>(null);

  async function check() {
    const r = await attempt(() => commands.checkPermissions());
    if (r.ok) {
      perms = r.value;
      error = null;
    } else error = r.error;
  }

  $effect(() => {
    void check();
    return everyWhileVisible(check, 1500);
  });

  async function open(pane: NonNullable<PermissionStatus_Serialize["pane"]>) {
    const r = await attempt(() => commands.openSystemSettings(pane));
    if (!r.ok) error = r.error;
  }

  const relevant = $derived((perms ?? []).filter((p) => p.state !== "not_applicable"));
  // El portal de Wayland no se puede conceder de antemano: el selector de pantalla aparece al
  // empezar a capturar, así que "todavía no se pidió" no bloquea el asistente (se avisa igual).
  const ok = (p: PermissionStatus_Serialize) =>
    p.state === "granted" || (p.kind === "screen_cast_portal" && p.state === "not_determined");
  $effect(() => {
    valid = perms !== null && relevant.every(ok);
  });
  const needsRestart = $derived(relevant.some((p) => p.needs_restart));
</script>

{#if error}
  <ErrorCard {error} retry={check} />
{/if}

{#if perms === null && !error}
  <p role="status" class="muted"><span class="spinner" aria-hidden="true"></span> {t("onboarding.permissions.checking")}</p>
{:else}
  <ul class="perms" aria-live="polite">
    {#each relevant as p (p.kind)}
      <li class="card" data-state={p.state}>
        <span class="state" aria-hidden="true"><Icon name={p.state === "granted" ? "check" : "alert"} size={18} /></span>
        <div class="text">
          <p class="h">{t(`permission.${p.kind}.title`)}</p>
          <p class="muted">{t(`permission.${p.kind}.why`)}</p>
        </div>
        <span class="chip" class:ok={p.state === "granted"} class:warn={p.state === "not_determined"} class:danger={p.state === "denied"}>{t(`permission.state.${p.state}`)}</span>
        {#if p.state !== "granted" && p.pane}
          <button type="button" class="btn primary small" onclick={() => open(p.pane!)}>
            <Icon name="sliders" size={16} />{t(`fix.open_settings.${p.pane}`)}
          </button>
        {/if}
        {#if p.kind === "screen_recording" && p.state === "denied"}
          <!-- En macOS puede figurar activado y no servir: quedó atado a otra versión de la app. -->
          <div class="repair">
            <p class="muted">{t("permission.repair.hint")}</p>
            <button type="button" class="btn ghost small" disabled={app.busy.repairScreen} onclick={() => repairScreenPermission()}>
              {#if app.busy.repairScreen}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="refresh" size={16} />{/if}
              {t("fix.repair_screen_permission")}
            </button>
          </div>
        {/if}
      </li>
    {:else}
      <li class="card"><span class="state"><Icon name="check" size={18} /></span><p>{t("onboarding.permissions.none_needed")}</p></li>
    {/each}
  </ul>
  <p class="auto muted"><span class="pulse" aria-hidden="true"></span>{t("onboarding.permissions.auto")}</p>
  {#if needsRestart}
    <p class="restart"><Icon name="alert" size={18} /> {t("onboarding.permissions.restart")}</p>
  {/if}
  <button type="button" class="btn ghost small" onclick={() => (app.help = "permissions")}><Icon name="help" size={16} />{t("onboarding.how")}</button>
{/if}

<style>
  .perms {
    list-style: none;
    margin: 0 0 1rem;
    padding: 0;
    display: grid;
    gap: 0.7rem;
  }
  li {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 0.9rem;
    align-items: center;
    padding: 1rem 1.1rem;
  }
  .repair {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    align-items: center;
    justify-content: space-between;
    border-top: 1px solid var(--line);
    padding-top: 0.7rem;
    font-size: 0.9rem;
  }
  .state {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: var(--warn-soft);
    color: var(--warn);
  }
  [data-state="granted"] .state {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .h {
    font-weight: 700;
  }
  .auto {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.88rem;
  }
  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.4s infinite;
  }
  .restart {
    display: flex;
    gap: 0.5rem;
    margin: 0.8rem 0;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    max-width: 70ch;
  }
  @media (max-width: 640px) {
    li {
      grid-template-columns: auto 1fr;
    }
  }
</style>
