<script lang="ts">
  import { app, init } from "./lib/app.svelte";
  import { route } from "./lib/router.svelte";
  import { i18n, t } from "./lib/i18n/index.svelte";
  import { applyTheme, theme } from "./lib/theme.svelte";
  import { hsvCss } from "./lib/format";
  import ErrorStack from "./lib/components/ErrorStack.svelte";
  import HelpDrawer from "./lib/components/HelpDrawer.svelte";
  import Rail from "./lib/components/Rail.svelte";
  import Onboarding from "./screens/Onboarding.svelte";
  import Dashboard from "./screens/Dashboard.svelte";
  import Settings from "./screens/Settings.svelte";
  import Devices from "./screens/Devices.svelte";
  import Diagnostics from "./screens/Diagnostics.svelte";
  import Manual from "./screens/Manual.svelte";
  import Calibration from "./screens/Calibration.svelte";
  import TrayMock from "./screens/TrayMock.svelte";

  $effect(() => {
    applyTheme(theme.value);
    void init();
  });
  $effect(() => {
    document.documentElement.lang = i18n.locale;
    document.title = route.name === "dashboard" ? "OpenLight" : `${t(`nav.${route.name}`)} · OpenLight`;
  });

  // La ventana misma se ilumina con el color que va al primer foco (el "ambilight" de la app).
  const sent = $derived.by(() => {
    const first = Object.values(app.live).find((l) => l.sent);
    return first?.sent && app.session === "active" ? first.sent : null;
  });
  // El halo usa cualquier color; los controles (sliders, interruptores) solo uno con cuerpo, para no
  // quedar blancos sobre blanco ni negros sobre negro.
  const ambient = $derived(sent ? hsvCss(sent) : null);
  const live = $derived(sent && (sent.s ?? 0) >= 0.3 && (sent.v ?? 0) >= 0.4 ? hsvCss(sent) : null);
  const fullscreen = $derived(route.name === "onboarding" || route.name === "calibrate");
</script>

<div class="shell" class:fullscreen class:idle={!ambient} style:--ambient={ambient} style:--live={live}>
  <a class="skip" href="#main">{t("common.skip_to_content")}</a>
  <div class="ambient" aria-hidden="true"></div>
  {#if !fullscreen}
    <Rail />
  {/if}
  <main id="main" tabindex="-1">
    {#if !app.ready}
      <div class="loading" role="status"><span class="spinner" aria-hidden="true"></span>{t("common.loading")}</div>
    {:else if route.name === "onboarding"}
      <Onboarding />
    {:else if route.name === "settings"}
      <Settings />
    {:else if route.name === "devices"}
      <Devices />
    {:else if route.name === "diagnostics"}
      <Diagnostics />
    {:else if route.name === "manual"}
      <Manual deviceId={route.param} />
    {:else if route.name === "calibrate"}
      <Calibration deviceId={route.param} />
    {:else if route.name === "tray"}
      <TrayMock />
    {:else}
      <Dashboard />
    {/if}
  </main>
</div>
<ErrorStack />
<HelpDrawer />

<style>
  .shell {
    position: relative;
    display: grid;
    grid-template-columns: var(--rail) 1fr;
    min-height: 100vh;
  }
  .shell.fullscreen {
    grid-template-columns: 1fr;
  }
  .ambient {
    position: fixed;
    inset: -20vmax;
    pointer-events: none;
    background:
      radial-gradient(55vmax 38vmax at 82% -4%, color-mix(in srgb, var(--ambient) 34%, transparent), transparent 70%),
      radial-gradient(40vmax 30vmax at 100% 40%, color-mix(in srgb, var(--ambient) 16%, transparent), transparent 70%);
    transition: opacity 600ms var(--ease);
    z-index: 0;
  }
  /* En reposo: mezcla suave de rojo, verde y azul. */
  .shell.idle .ambient {
    background:
      radial-gradient(46vmax 32vmax at 92% -6%, color-mix(in srgb, var(--rgb-r) 22%, transparent), transparent 70%),
      radial-gradient(44vmax 32vmax at 62% -14%, color-mix(in srgb, var(--rgb-g) 18%, transparent), transparent 70%),
      radial-gradient(48vmax 34vmax at 100% 34%, color-mix(in srgb, var(--rgb-b) 20%, transparent), transparent 70%);
  }
  main {
    position: relative;
    z-index: 1;
    min-width: 0;
    outline: none;
  }
  .loading {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    color: var(--muted);
  }
  .skip {
    position: absolute;
    left: 1rem;
    top: -4rem;
    z-index: 60;
    padding: 0.6rem 1rem;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 700;
  }
  .skip:focus {
    top: 1rem;
  }
  @media (max-width: 820px) {
    .shell {
      grid-template-columns: 1fr;
    }
  }
</style>
