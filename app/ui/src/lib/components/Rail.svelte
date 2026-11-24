<script lang="ts">
  import { app, toggleSession } from "../app.svelte";
  import { go, route, type RouteName } from "../router.svelte";
  import { i18n, locales, setLocale, t, type Locale } from "../i18n/index.svelte";
  import { applyTheme, theme, type Theme } from "../theme.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import logoDark from "../../assets/logo-dark.svg";
  import logoLight from "../../assets/logo-light.svg";

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
    <img class="logo light" src={logoLight} alt="OpenLight" />
    <img class="logo dark" src={logoDark} alt="OpenLight" />
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
    background: var(--surface);
    border-right: 1px solid var(--card-line);
    border-radius: 0 var(--radius) var(--radius) 0;
    box-shadow: var(--shadow);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0 0.5rem;
  }
  /* El logo trae dos versiones: la oscura para tema oscuro y la clara para tema claro. */
  .logo {
    height: 36px;
    width: auto;
    max-width: 100%;
  }
  .logo.dark {
    display: none;
  }
  :global(:root[data-theme="dark"]) .logo.light {
    display: none;
  }
  :global(:root[data-theme="dark"]) .logo.dark {
    display: block;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .logo.light {
      display: none;
    }
    :global(:root:not([data-theme="light"])) .logo.dark {
      display: block;
    }
  }
  .session {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.1rem 0.7rem;
    align-items: center;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-md);
    border: 0;
    background: var(--surface-2);
    color: var(--text);
    font: 700 0.95rem var(--font-body);
    text-align: left;
    cursor: pointer;
    transition: background var(--dur) var(--ease);
  }
  .session:hover {
    background: var(--surface-3);
  }
  .session.on {
    background: linear-gradient(135deg, color-mix(in srgb, var(--ambient, var(--rgb-b)) 30%, var(--surface-2)), var(--surface-2));
  }
  .session :global(svg) {
    color: var(--muted);
  }
  .session.on :global(svg) {
    color: var(--text);
  }
  .session .state {
    grid-column: 2;
    font: 500 0.78rem var(--font-body);
    color: var(--muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
  }
  a {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 0.7rem 1rem;
    border-radius: 999px;
    color: var(--muted);
    text-decoration: none;
    font-weight: 600;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  a:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  /* El ítem activo es la pastilla negra de la barra de la referencia. */
  a[aria-current="page"] {
    color: var(--accent-ink);
    background: var(--accent);
  }
  .prefs {
    margin-top: auto;
    display: grid;
    gap: 0.5rem;
  }
  .seg {
    display: flex;
    padding: 4px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .seg button {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 32px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: 700 0.78rem var(--font-body);
    cursor: pointer;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .seg button[aria-pressed="true"] {
    background: var(--accent);
    color: var(--accent-ink);
  }
  @media (max-width: 820px) {
    .rail {
      position: static;
      height: auto;
      flex-direction: row;
      flex-wrap: wrap;
      align-items: center;
      border-radius: 0 0 var(--radius-md) var(--radius-md);
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
    a {
      padding: 0.7rem;
    }
    .prefs {
      margin: 0;
      display: flex;
    }
  }
</style>
