<script lang="ts">
  // Errores globales (eventos `error://raised` y fallas de comandos sin un lugar propio).
  import { app, dismiss } from "../app.svelte";
  import { t } from "../i18n/index.svelte";
  import { route } from "../router.svelte";
  import ErrorCard from "./ErrorCard.svelte";
  import RepairNotice from "./RepairNotice.svelte";

  // En el panel, los errores de cada foco se muestran en su tarjeta.
  const shown = $derived(app.errors.filter((e) => !(route.name === "dashboard" && e.params.device && app.devices.some((d) => d.id === e.params.device))));
</script>

<!-- En la calibración el panel está abajo y en el tray el menú baja hasta el borde: los avisos van arriba para no taparlos. -->
<section class="stack" class:top={route.name === "calibrate" || route.name === "tray"} aria-label={t("common.notifications")}>
  {#if app.screenRepaired}
    <div class="rise">
      <RepairNotice />
    </div>
  {/if}
  {#each shown as e (e.uid)}
    <div class="rise">
      <ErrorCard error={e} retry={e.retry} ondismiss={() => dismiss(e.uid)} onresolved={() => dismiss(e.uid)} />
    </div>
  {/each}
</section>

<style>
  .stack {
    position: fixed;
    right: 1.25rem;
    bottom: 1.25rem;
    z-index: 50;
    display: grid;
    gap: 0.6rem;
    width: min(420px, calc(100vw - 2.5rem));
    max-height: calc(100vh - 2.5rem);
    overflow: auto;
  }
  .stack.top {
    bottom: auto;
    top: 1.25rem;
  }
  .stack:empty {
    display: none;
  }
</style>
