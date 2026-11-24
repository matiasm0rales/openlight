<script lang="ts">
  // Paso 11: arrancar, abrir al iniciar sesión (plugin autostart) y dónde queda el ícono.
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { app, raise, toggleSession } from "../../lib/app.svelte";
  import { go } from "../../lib/router.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Switch from "../../lib/components/Switch.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();
  let autostart = $state(false);
  let busy = $state(false);

  $effect(() => {
    valid = true;
    isEnabled().then((v) => (autostart = v)).catch(() => (autostart = false));
  });

  async function setAutostart(v: boolean) {
    busy = true;
    try {
      await (v ? enable() : disable());
      autostart = v;
    } catch (e) {
      raise(e);
    }
    busy = false;
  }

  async function start() {
    if (app.session !== "active") await toggleSession();
    go("dashboard");
  }

  const os = $derived(navigator.userAgent.includes("Mac") ? "mac" : navigator.userAgent.includes("Win") ? "windows" : "linux");
</script>

<div class="done">
  <div class="hero" aria-hidden="true">
    {#each app.devices as d, i (d.id)}<span class="orb" style:animation-delay="{i * 240}ms"></span>{/each}
  </div>
  <div class="row card">
    <Switch checked={autostart} label={t("onboarding.done.autostart")} onchange={setAutostart} {busy} id="autostart" />
    <label for="autostart" class="lbl">{t("onboarding.done.autostart")}</label>
  </div>
  <p class="tray card"><Icon name="tray" size={20} /> <span>{t(`onboarding.done.tray_${os}`)}</span></p>
  <div class="actions">
    <button type="button" class="btn primary big" onclick={start} data-testid="start"><Icon name="power" size={18} />{t("onboarding.done.start")}</button>
    <button type="button" class="btn ghost" onclick={() => go("dashboard")}>{t("onboarding.done.later")}</button>
  </div>
</div>

<style>
  .done {
    display: grid;
    gap: 1rem;
    max-width: 620px;
  }
  .hero {
    display: flex;
    gap: 1.2rem;
    padding: 1rem 0 1.4rem;
  }
  .orb {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, #fff6, transparent 40%), conic-gradient(from 90deg, #ff6a4b, #ffb547, #6be3a4, #59b8ff, #b07cff, #ff6a4b);
    box-shadow: 0 0 56px -2px var(--rgb-b);
    animation: rise 700ms var(--ease) both;
  }
  .row {
    display: flex;
    gap: 1rem;
    align-items: center;
    padding: 1rem;
  }
  .lbl {
    font-weight: 700;
    cursor: pointer;
  }
  .tray {
    display: flex;
    gap: 0.7rem;
    padding: 1rem;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 0.8rem;
    margin-top: 0.6rem;
  }
  .big {
    min-height: 52px;
    padding: 0 1.6rem;
    font-size: 1.05rem;
  }
</style>
