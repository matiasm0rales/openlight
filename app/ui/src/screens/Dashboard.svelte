<script lang="ts">
  // B. Panel principal: interruptor grande, perfil activo y una tarjeta por foco, en el orden que
  // elija el usuario (el mismo que ven Dispositivos y el tray).
  import { tick } from "svelte";
  import { activateProfile, app, reorderDevices, toggleSession } from "../lib/app.svelte";
  import { dropAt, move } from "../lib/order";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { ONBOARDING_STEPS } from "../lib/wizard.svelte";
  import DeviceCard from "../lib/components/DeviceCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import ReorderHandle from "../lib/components/ReorderHandle.svelte";
  import Switch from "../lib/components/Switch.svelte";
  import UpdateCard from "../lib/components/UpdateCard.svelte";
  import { noticeVisible } from "../lib/updater.svelte";

  const running = $derived(app.session === "active" || app.session === "connecting" || app.session === "paused");
  const pending = $derived(app.onboarding && app.onboarding.step !== "done" ? ONBOARDING_STEPS.length - app.onboarding.completed.length : 0);
  const profileName = (name: string) => (name.startsWith("profile.") ? t(name) : name);

  // ---------- orden de las tarjetas ----------

  let list: HTMLUListElement | undefined = $state();
  /** El foco que se arrastra y el lugar donde caería. */
  let drag = $state<{ id: string; over: number | null } | null>(null);
  const dragName = $derived(app.devices.find((d) => d.id === drag?.id)?.name ?? "");
  // La entrada animada es solo al abrir: mover una tarjeta de lugar la volvería a disparar.
  let moved = $state(false);
  let status = $state("");

  /** Lo que oye quien usa un lector de pantalla. Se vacía antes para que repita un mismo texto. */
  async function announce(text: string) {
    status = "";
    await tick();
    status = text;
  }

  // Cambiar de lugar una tarjeta saca el foco del teclado de lo que tenga adentro: se devuelve.
  async function keepFocus(on: Element | null) {
    await tick();
    if (on instanceof HTMLElement && on.isConnected && (document.activeElement ?? document.body) === document.body) on.focus();
  }

  async function place(id: string, to: number) {
    const from = app.devices.findIndex((d) => d.id === id);
    if (from < 0) return;
    const name = app.devices[from].name;
    if (to < 0) return announce(t("dashboard.already_first", { name }));
    if (to >= app.devices.length) return announce(t("dashboard.already_last", { name }));
    moved = true;
    const focused = document.activeElement;
    const saved = reorderDevices(move(app.devices.map((d) => d.id), from, to));
    void announce(t("dashboard.moved", { name, n: to + 1, total: app.devices.length }));
    await keepFocus(focused);
    if (await saved) return;
    await keepFocus(focused);
    await announce(t("dashboard.move_failed"));
  }

  function aim(id: string, x: number, y: number) {
    const from = app.devices.findIndex((d) => d.id === id);
    const boxes = Array.from(list?.children ?? [], (card) => card.getBoundingClientRect());
    drag = { id, over: dropAt(boxes, from, x, y) };
  }

  function drop() {
    const dropped = drag;
    drag = null;
    if (dropped && dropped.over !== null) void place(dropped.id, dropped.over);
  }
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

  {#if noticeVisible()}<UpdateCard />{/if}

  {#if pending > 0}
    <div class="resume card">
      <Icon name="target" size={20} />
      <p>{t("dashboard.resume", { n: pending })}</p>
      <button type="button" class="btn primary small" onclick={() => go("onboarding")}>{t("dashboard.resume_cta")}</button>
    </div>
  {/if}

  <p id="reorder-status" class="sr-only" aria-live="polite" aria-atomic="true">{status}</p>

  {#if app.devices.length}
    <p id="reorder-help" hidden>{t("dashboard.reorder_help")}</p>
    <ul class="grid" class:sorting={drag} aria-label={t("dashboard.bulbs")} bind:this={list}>
      {#each app.devices as d, i (d.id)}
        <li class:rise={!moved} class:lifted={drag?.id === d.id} style:animation-delay="{i * 90}ms">
          <DeviceCard device={d}>
            {#snippet lead()}
              {#if app.devices.length > 1}
                <ReorderHandle
                  id={d.id}
                  name={d.name}
                  first={i === 0}
                  last={i === app.devices.length - 1}
                  describedby="reorder-help"
                  onmove={(step) => void place(d.id, app.devices.findIndex((x) => x.id === d.id) + step)}
                  ondrag={(x, y) => aim(d.id, x, y)}
                  ondrop={drop}
                  oncancel={() => (drag = null)}
                />
              {/if}
            {/snippet}
          </DeviceCard>
          {#if drag?.over === i}
            <span class="landing" aria-hidden="true"><span>{t("dashboard.drop_here", { name: dragName })}</span></span>
          {/if}
        </li>
      {/each}
    </ul>
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
    padding-bottom: 0.4rem;
  }
  .title {
    display: grid;
    gap: 0.5rem;
  }
  h1 {
    font-size: clamp(2.2rem, 4.6vw, 3.6rem);
    font-weight: 650;
    letter-spacing: -0.035em;
  }
  .power {
    display: grid;
    justify-items: center;
    gap: 0.4rem;
  }
  .state {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--muted);
  }
  .profile {
    display: grid;
    gap: 0.35rem;
    min-width: 210px;
  }
  .profile label {
    font-weight: 600;
    font-size: 0.85rem;
  }
  .resume {
    display: flex;
    gap: 0.8rem;
    align-items: center;
    padding: 0.8rem 1.1rem;
    border-radius: var(--radius-md);
  }
  .resume p {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 420px), 1fr));
    gap: 1.1rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .grid li {
    position: relative;
  }
  .grid.sorting {
    -webkit-user-select: none;
    user-select: none;
  }
  /* La que se está moviendo queda "levantada": sin sombra y punteada, con el texto igual de legible. */
  .lifted > :global(.device) {
    background: var(--surface-2);
    box-shadow: none;
    outline: 2px dashed var(--line-strong);
    outline-offset: -2px;
  }
  /* Dónde cae lo que se arrastra: el lugar entero, marcado y con su nombre. */
  .landing {
    position: absolute;
    inset: -4px;
    z-index: 4;
    display: grid;
    place-items: center;
    border: 3px dashed var(--focus);
    border-radius: calc(var(--radius) + 4px);
    background: color-mix(in srgb, var(--focus) 12%, transparent);
    pointer-events: none;
  }
  .landing span {
    padding: 0.45em 1.1em;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 700;
    font-size: 0.9rem;
    box-shadow: var(--shadow);
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
