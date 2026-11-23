<script lang="ts">
  import { app, toggleSession } from "../app.svelte";
  import { go, route, type RouteName } from "../router.svelte";
  import { i18n, locales, setLocale, t, type Locale } from "../i18n/index.svelte";
  import { applyTheme, theme, type Theme } from "../theme.svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  const items: { name: RouteName; icon: IconName }[] = [
    { name: "dashboard", icon: "home" },
    { name: "settings", icon: "sliders" },
    { name: "devices", icon: "devices" },
    { name: "diagnostics", icon: "pulse" },
    { name: "tray", icon: "tray" },
  ];
  const running = $derived(app.session === "active" || app.session === "connecting" || app.session === "paused");
  const themes: { v: Theme; icon: IconName }[] = [
    { v: "system", icon: "monitor" },
    { v: "light", icon: "sun" },
    { v: "dark", icon: "moon" },
  ];
</script>

<nav class="rail" aria-label={t("nav.label")}>
  <div class="brand">
    <span class="logo" aria-hidden="true"></span>
    <span class="word">Open<em>Light</em></span>
  </div>

  <button type="button" class="session" class:on={running} onclick={toggleSession} disabled={app.busy.session} aria-pressed={running}>
    <Icon name="power" size={18} />
    <span>{running ? t("session.stop") : t("session.start")}</span>
    <span class="state">{t(`session.state.${app.session}`)}</span>
  </button>

  <ul>
    {#each items as item (item.name)}
      <li>
        <a href="#/{item.name}" aria-current={route.name === item.name ? "page" : undefined} onclick={(e) => { e.preventDefault(); go(item.name); }}>
          <Icon name={item.icon} />
          <span>{t(`nav.${item.name}`)}</span>
        </a>
      </li>
    {/each}
  </ul>

  <div class="prefs">
    <div class="seg" role="group" aria-label={t("prefs.theme")}>
      {#each themes as th (th.v)}
        <button type="button" aria-pressed={theme.value === th.v} aria-label={t(`prefs.theme_${th.v}`)} title={t(`prefs.theme_${th.v}`)} onclick={() => applyTheme(th.v)}>
          <Icon name={th.icon} size={16} />
        </button>
      {/each}
    </div>
    <div class="seg" role="group" aria-label={t("prefs.language")}>
      {#each locales() as l (l)}
        <button type="button" lang={l} aria-pressed={i18n.locale === l} onclick={() => setLocale(l as Locale)}>{l.toUpperCase()}</button>
      {/each}
    </div>
  </div>
</nav>

<style>
  .rail {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 1.4rem;
    height: 100vh;
    padding: 1.4rem 1rem;
    border-right: 1px solid var(--line);
    background: color-mix(in srgb, var(--bg-deep) 80%, transparent);
    backdrop-filter: blur(8px);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0 0.4rem;
  }
  .logo {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 50%, var(--accent) 0 32%, transparent 34%), conic-gradient(from 200deg, #ff6a4b, #ffb547, #f8f06a, #6be3a4, #59b8ff, #b07cff, #ff6a4b);
    box-shadow: 0 0 18px var(--glow);
  }
  .word {
    font: 600 1.25rem/1 var(--font-display);
    font-variation-settings: "SOFT" 100, "opsz" 48;
  }
  .word em {
    font-style: italic;
    color: var(--accent);
  }
  .session {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.15rem 0.6rem;
    align-items: center;
    padding: 0.75rem 0.9rem;
    border-radius: var(--radius);
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    font: 700 0.95rem var(--font-body);
    text-align: left;
    cursor: pointer;
  }
  .session.on {
    border-color: var(--accent);
    background: linear-gradient(140deg, var(--accent-soft), transparent), var(--surface);
  }
  .session.on :global(svg) {
    color: var(--accent);
  }
  .session .state {
    grid-column: 2;
    font: 500 0.72rem var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.15rem;
  }
  a {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.6rem 0.75rem;
    border-radius: var(--radius-sm);
    color: var(--muted);
    text-decoration: none;
    font-weight: 700;
  }
  a:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  a[aria-current="page"] {
    color: var(--text);
    background: var(--surface-2);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .prefs {
    margin-top: auto;
    display: grid;
    gap: 0.5rem;
  }
  .seg {
    display: flex;
    padding: 3px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .seg button {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 30px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: 700 0.78rem var(--font-mono);
    cursor: pointer;
  }
  .seg button[aria-pressed="true"] {
    background: var(--surface-3);
    color: var(--text);
  }
  @media (max-width: 820px) {
    .rail {
      position: static;
      height: auto;
      flex-direction: row;
      flex-wrap: wrap;
      align-items: center;
      border-right: 0;
      border-bottom: 1px solid var(--line);
    }
    ul {
      display: flex;
      flex-wrap: wrap;
    }
    a span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
      white-space: nowrap;
    }
    .prefs {
      margin: 0;
      display: flex;
    }
  }
</style>
