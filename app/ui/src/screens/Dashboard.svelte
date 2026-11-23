<script lang="ts">
  // B. Panel principal: interruptor grande, perfil activo y una tarjeta por foco.
  import { activateProfile, app, toggleSession } from "../lib/app.svelte";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { ONBOARDING_STEPS } from "../lib/wizard.svelte";
  import DeviceCard from "../lib/components/DeviceCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Switch from "../lib/components/Switch.svelte";

  const running = $derived(app.session === "active" || app.session === "connecting" || app.session === "paused");
  const pending = $derived(app.onboarding && app.onboarding.step !== "done" ? ONBOARDING_STEPS.length - app.onboarding.completed.length : 0);
  const profileName = (name: string) => (name.startsWith("profile.") ? t(name) : name);
</script>

<div class="page">
  <section class="hero">
    <div class="title">
      <span class="eyebrow">{t("dashboard.eyebrow")}</span>
      <h1>{running ? t("dashboard.title_on") : t("dashboard.title_off")}</h1>
      <p class="muted">{t(`session.explain.${app.session}`)}</p>
    </div>
    <div class="power">
      <Switch big checked={running} label={running ? t("session.stop") : t("session.start")} busy={Boolean(app.busy.session)} onchange={toggleSession} />
      <span class="state mono">{t(`session.state.${app.session}`)}</span>
    </div>
    <div class="profile">
      <label for="profile">{t("dashboard.profile")}</label>
      <select id="profile" value={app.settings?.active_profile ?? ""} onchange={(e) => activateProfile(e.currentTarget.value || null)}>
        <option value="">{t("dashboard.no_profile")}</option>
        {#each app.profiles as p (p.id)}<option value={p.id}>{profileName(p.name)}{p.shortcut ? ` (${p.shortcut})` : ""}</option>{/each}
      </select>
    </div>
  </section>

  {#if pending > 0}
    <div class="resume card">
      <Icon name="target" size={20} />
      <p>{t("dashboard.resume", { n: pending })}</p>
      <button type="button" class="btn primary small" onclick={() => go("onboarding")}>{t("dashboard.resume_cta")}</button>
    </div>
  {/if}

  {#if app.devices.length}
    <div class="grid">
      {#each app.devices as d, i (d.id)}
        <div class="rise" style:animation-delay="{i * 90}ms"><DeviceCard device={d} /></div>
      {/each}
    </div>
  {:else}
    <div class="empty card">
      <Icon name="bulb" size={34} />
      <h2>{t("dashboard.empty_title")}</h2>
      <p class="muted">{t("dashboard.empty_text")}</p>
      <button type="button" class="btn primary" onclick={() => go("onboarding")}>{t("dashboard.empty_cta")}</button>
    </div>
  {/if}
</div>

<style>
  .page {
    display: grid;
    gap: 1.6rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 1240px;
  }
  .hero {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 2rem;
    align-items: end;
    padding-bottom: 1.4rem;
    border-bottom: 1px solid var(--line);
  }
  .title {
    display: grid;
    gap: 0.5rem;
  }
  h1 {
    font-size: clamp(2.2rem, 4.6vw, 3.6rem);
    font-variation-settings: "SOFT" 100, "opsz" 144;
    font-weight: 480;
  }
  .power {
    display: grid;
    justify-items: center;
    gap: 0.4rem;
  }
  .state {
    font-size: 0.72rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .profile {
    display: grid;
    gap: 0.35rem;
    min-width: 210px;
  }
  .profile label {
    font-weight: 700;
    font-size: 0.85rem;
  }
  .resume {
    display: flex;
    gap: 0.8rem;
    align-items: center;
    padding: 0.8rem 1rem;
    border-color: var(--accent);
  }
  .resume p {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 420px), 1fr));
    gap: 1.1rem;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 0.7rem;
    padding: 3rem 1.5rem;
    text-align: center;
    color: var(--muted);
  }
  .empty h2 {
    color: var(--text);
  }
  @media (max-width: 900px) {
    .hero {
      grid-template-columns: 1fr;
      align-items: start;
    }
    .power {
      justify-items: start;
    }
  }
</style>
