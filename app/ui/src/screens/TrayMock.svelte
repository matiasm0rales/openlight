<script lang="ts">
  // Mock del menú del tray / barra de menú (el real lo arma src-tauri en I1 con los mismos comandos):
  // encender/apagar, perfil, foco por foco, abrir y salir (salir restaura los focos).
  // Abajo, solo con el backend falso, un panel para forzar escenarios.
  import { activateProfile, app, refreshDevices, refreshOnboarding, refreshSettings, toggleSession } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { hsvCss } from "../lib/format";
  import type { MockBackend } from "../lib/mocks/backend";
  import Icon from "../lib/components/Icon.svelte";

  const running = $derived(app.session === "active" || app.session === "connecting" || app.session === "paused");
  const mock = (window as unknown as { __openlightMock?: MockBackend }).__openlightMock;
  let quitNote = $state(false);

  async function quit() {
    await attempt(() => commands.sessionStop());
    quitNote = true;
  }

  async function scenario(s: "fresh" | "configured") {
    mock?.reset(s);
    app.health = {};
    app.live = {};
    app.errors = [];
    app.stats = [];
    app.session = "idle";
    await Promise.all([refreshOnboarding(), refreshDevices(), refreshSettings()]);
    go(s === "fresh" ? "onboarding" : "dashboard");
  }
  const profileName = (n: string) => (n.startsWith("profile.") ? t(n) : n);
</script>

<div class="page">
  <header>
    <span class="eyebrow">{t("nav.tray")}</span>
    <h1>{t("tray.title")}</h1>
    <p class="muted">{t("tray.explain")}</p>
  </header>

  <div class="desk">
    <div class="menubar" aria-hidden="true"><span class="logo"></span><span class="clock">21:47</span></div>
    <div class="menu" role="group" aria-label={t("tray.menu_label")}>
      <p class="status"><span class="dot" class:on={running}></span>OpenLight — {t(`session.state.${app.session}`)}</p>
      <button type="button" onclick={toggleSession}><Icon name="power" size={16} />{running ? t("session.stop") : t("session.start")}</button>
      <div class="sep" aria-hidden="true"></div>
      <p class="group">{t("dashboard.profile")}</p>
      <button type="button" aria-pressed={!app.settings?.active_profile} onclick={() => activateProfile(null)}><span class="check">{!app.settings?.active_profile ? "✓" : ""}</span>{t("dashboard.no_profile")}</button>
      {#each app.profiles as p (p.id)}
        <button type="button" aria-pressed={app.settings?.active_profile === p.id} onclick={() => activateProfile(p.id)}>
          <span class="check">{app.settings?.active_profile === p.id ? "✓" : ""}</span>{profileName(p.name)}{#if p.shortcut}<kbd>{p.shortcut}</kbd>{/if}
        </button>
      {/each}
      <div class="sep" aria-hidden="true"></div>
      <p class="group">{t("tray.devices")}</p>
      {#each app.devices as d (d.id)}
        {@const st = app.health[d.id]?.state}
        <button type="button" onclick={() => (st?.kind === "error" && st.code === "device_key_rejected" ? go("devices") : go("manual", d.id))}>
          <span class="sw" style:background={app.live[d.id]?.sent ? hsvCss(app.live[d.id].sent) : "var(--surface-3)"}></span>
          {d.name}<span class="sub">{st?.kind === "error" ? t(`error.${st.code}.title`) : t(`device_state.${st?.kind ?? "idle"}`)}</span>
        </button>
      {/each}
      <div class="sep" aria-hidden="true"></div>
      <button type="button" onclick={() => go("dashboard")}><Icon name="home" size={16} />{t("tray.open")}</button>
      <button type="button" onclick={quit}><Icon name="close" size={16} />{t("tray.quit")}</button>
    </div>
    {#if quitNote}<p class="note" role="status">{t("tray.quit_note")}</p>{/if}
  </div>

  {#if mock}
    <section class="card scenarios" aria-labelledby="sc-title">
      <h2 id="sc-title">{t("tray.mock_title")}</h2>
      <p class="muted">{t("tray.mock_help")}</p>
      <div class="row">
        <button type="button" class="btn small" onclick={() => scenario("fresh")}>{t("tray.mock_fresh")}</button>
        <button type="button" class="btn small" onclick={() => scenario("configured")}>{t("tray.mock_configured")}</button>
        <button type="button" class="btn small" onclick={() => mock.inject("key_rejected")}>{t("tray.mock_914")}</button>
        <button type="button" class="btn small" onclick={() => mock.inject("disconnect")}>{t("tray.mock_drop")}</button>
        <button type="button" class="btn small" onclick={() => mock.inject("no_display")}>{t("tray.mock_no_display")}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .page {
    display: grid;
    gap: 1.4rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 900px;
  }
  header {
    display: grid;
    gap: 0.5rem;
  }
  .desk {
    position: relative;
    min-height: 460px;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: radial-gradient(120% 80% at 70% 0%, var(--accent-soft), transparent), var(--bg-deep);
    overflow: hidden;
  }
  .menubar {
    display: flex;
    justify-content: flex-end;
    gap: 1rem;
    align-items: center;
    height: 28px;
    padding: 0 0.9rem;
    background: rgb(0 0 0 / 0.3);
    font: 0.8rem var(--font-mono);
  }
  .logo {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    box-shadow: 0 0 8px var(--accent);
  }
  .menu {
    position: absolute;
    right: 3rem;
    top: 34px;
    width: 290px;
    padding: 0.35rem;
    border-radius: 10px;
    border: 1px solid var(--line-strong);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    backdrop-filter: blur(12px);
    box-shadow: 0 24px 50px -20px rgb(0 0 0 / 0.7);
  }
  .menu button {
    display: flex;
    gap: 0.55rem;
    align-items: center;
    width: 100%;
    padding: 0.4rem 0.6rem;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font: 0.9rem var(--font-body);
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--accent);
    color: var(--accent-ink);
    outline: none;
  }
  .menu kbd {
    margin-left: auto;
    font-size: 0.7rem;
    background: transparent;
    color: inherit;
  }
  .status {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.4rem 0.6rem;
    font-weight: 700;
    font-size: 0.85rem;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
  }
  .dot.on {
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok);
  }
  .group {
    padding: 0.3rem 0.6rem 0.1rem;
    font: 500 0.7rem var(--font-mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .check {
    width: 1em;
  }
  .sw {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .sub {
    margin-left: auto;
    font-size: 0.75rem;
    opacity: 0.8;
  }
  .sep {
    height: 1px;
    margin: 0.3rem 0.4rem;
    background: var(--line);
  }
  .note {
    position: absolute;
    left: 1rem;
    bottom: 1rem;
    max-width: 320px;
    padding: 0.7rem 0.9rem;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .scenarios {
    display: grid;
    gap: 0.6rem;
    padding: 1.1rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
</style>
