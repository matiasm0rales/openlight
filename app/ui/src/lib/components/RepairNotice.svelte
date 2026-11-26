<script lang="ts">
  // Después de «Reparar permiso»: lo que falta lo hace la persona (activarlo y reabrir la app).
  import { dismissRepairNotice, raise } from "../app.svelte";
  import { attempt, commands } from "../ipc";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  async function openSettings() {
    const r = await attempt(() => commands.openSystemSettings("screen_recording"));
    if (!r.ok) raise(r.error);
  }
</script>

<div class="notice" role="status">
  <span class="mark" aria-hidden="true"><Icon name="check" size={18} /></span>
  <div>
    <p class="title">{t("permission.repair.title")}</p>
    <p class="detail">{t("permission.repair.detail")}</p>
    <button type="button" class="btn small primary" onclick={openSettings}>
      <Icon name="sliders" size={16} />{t("fix.open_settings.screen_recording")}
    </button>
  </div>
  <button type="button" class="close btn ghost small" aria-label={t("common.dismiss")} onclick={dismissRepairNotice}>
    <Icon name="close" size={16} />
  </button>
</div>

<style>
  .notice {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.75rem;
    padding: 0.95rem 1rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--line);
    background: linear-gradient(160deg, var(--accent-soft), transparent 70%), var(--surface);
    box-shadow: var(--shadow);
  }
  .mark {
    color: var(--ok);
    padding-top: 0.15rem;
  }
  .title {
    font-weight: 700;
  }
  .detail {
    color: var(--muted);
    margin: 0.15rem 0 0.7rem;
  }
  .close {
    align-self: start;
    width: 32px;
    padding: 0;
  }
</style>
