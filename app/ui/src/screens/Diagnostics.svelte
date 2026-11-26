<script lang="ts">
  // G. Diagnóstico: fps capturados y envíos por segundo, salud de cada enlace con el motivo del
  // último corte, visor de log y "Exportar diagnóstico" (zip sin secretos).
  import type { LogLevel, LogLine, UserFacingError } from "../lib/bindings";
  import { app } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { i18n, t } from "../lib/i18n/index.svelte";
  import { formatAgo, formatBytes, formatTime } from "../lib/format";
  import { everyWhileVisible } from "../lib/visibility";
  import Chart from "../lib/components/Chart.svelte";
  import ErrorCard from "../lib/components/ErrorCard.svelte";
  import Icon from "../lib/components/Icon.svelte";

  const COLORS = ["var(--accent)", "#59b8ff", "#82d69e", "#ff8a7a", "#c79bff"];
  const LEVELS: LogLevel[] = ["error", "warn", "info", "debug"];

  let log = $state<LogLine[]>([]);
  let levels = $state<LogLevel[]>(["error", "warn", "info"]);
  let query = $state("");
  let follow = $state(true);
  let error = $state<UserFacingError | null>(null);
  let exported = $state<string | null>(null);
  let exporting = $state(false);
  let viewer: HTMLDivElement | undefined = $state();

  async function loadLog() {
    const r = await attempt(() => commands.diagnosticsLog(300));
    if (r.ok) log = r.value;
    else error = r.error;
  }

  $effect(() => {
    void loadLog();
    return everyWhileVisible(loadLog, 3000);
  });

  const filtered = $derived(log.filter((l) => levels.includes(l.level) && (!query || `${l.target} ${l.message}`.toLowerCase().includes(query.toLowerCase()))));

  $effect(() => {
    void filtered.length;
    if (follow && viewer) queueMicrotask(() => viewer && (viewer.scrollTop = viewer.scrollHeight));
  });

  async function exportZip() {
    exporting = true;
    exported = null;
    const r = await attempt(() => commands.diagnosticsExport(null));
    exporting = false;
    if (r.ok) exported = r.value;
    else error = r.error;
  }

  const fps = $derived([{ label: t("diagnostics.captured"), color: COLORS[0], values: app.stats.map((s) => s.capture_fps) }]);
  const sends = $derived(app.devices.map((d, i) => ({ label: d.name, color: COLORS[(i + 1) % COLORS.length], values: app.stats.map((s) => s.sends[d.id] ?? 0) })));
  const target = $derived(Number(app.settings?.effective.capture_fps ?? 20));
  const maxSends = $derived(Number(app.settings?.effective.max_sends_per_s ?? 8));
</script>

<div class="page">
  <header class="head">
    <div>
      <span class="eyebrow">{t("nav.diagnostics")}</span>
      <h1>{t("diagnostics.title")}</h1>
    </div>
    <button type="button" class="btn primary" onclick={exportZip} disabled={exporting}>
      {#if exporting}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="download" size={16} />{/if}{t("diagnostics.export")}
    </button>
  </header>
  {#if exported}<p class="ok" role="status"><Icon name="check" size={16} />{t("diagnostics.exported", { path: exported })}</p>{/if}
  {#if error}<ErrorCard {error} retry={() => { error = null; return loadLog(); }} />{/if}

  <section class="charts" aria-labelledby="charts-title">
    <h2 id="charts-title" class="sr-only">{t("diagnostics.charts")}</h2>
    {#if app.session !== "active" && app.stats.length === 0}
      <p class="muted card pad">{t("diagnostics.no_session")}</p>
    {:else}
      <div class="card pad"><Chart title={t("diagnostics.fps_title", { target })} series={fps} unit="fps" max={target} /></div>
      <div class="card pad"><Chart title={t("diagnostics.sends_title", { max: maxSends })} series={sends} unit="/s" max={maxSends} /></div>
    {/if}
  </section>

  <section aria-labelledby="health-title">
    <h2 id="health-title">{t("diagnostics.health")}</h2>
    <div class="table card">
      <table>
        <thead>
          <tr>
            <th scope="col">{t("diagnostics.col.device")}</th>
            <th scope="col">{t("diagnostics.col.state")}</th>
            <th scope="col">{t("dashboard.latency")}</th>
            <th scope="col">{t("diagnostics.col.traffic")}</th>
            <th scope="col">{t("diagnostics.col.reconnects")}</th>
            <th scope="col">{t("dashboard.drops")}</th>
            <th scope="col">{t("dashboard.last_drop")}</th>
          </tr>
        </thead>
        <tbody>
          {#each app.devices as d (d.id)}
            {@const h = app.health[d.id]}
            <tr>
              <th scope="row">{d.name}</th>
              <td>{t(`device_state.${h?.state.kind ?? "idle"}`)}</td>
              <td class="tabular">{h?.health.rtt_ms != null ? `${h.health.rtt_ms} ms` : "—"}</td>
              <td class="tabular">↓ {formatBytes(h?.health.bytes_rx ?? 0, i18n.locale)} · ↑ {formatBytes(h?.health.bytes_tx ?? 0, i18n.locale)}</td>
              <td class="tabular">{h?.health.reconnects ?? 0}</td>
              <td class="tabular">{h?.health.disconnects_last_hour ?? 0}</td>
              <td>
                {#if h?.health.last_disconnect}
                  <strong>{t(`disconnect.${h.health.last_disconnect.kind}`)}</strong>
                  <span class="muted"> · {formatAgo(h.health.last_disconnect_at_ms ?? Date.now(), i18n.locale)}</span>
                  <p class="muted small">{t(`disconnect_help.${h.health.last_disconnect.kind}`)}</p>
                {:else}—{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section aria-labelledby="log-title" class="logs">
    <div class="loghead">
      <h2 id="log-title">{t("diagnostics.log")}</h2>
      <fieldset class="levels">
        <legend class="sr-only">{t("diagnostics.levels")}</legend>
        {#each LEVELS as l (l)}
          <label class="lvl" data-level={l}><input type="checkbox" value={l} bind:group={levels} />{t(`log_level.${l}`)}</label>
        {/each}
      </fieldset>
      <div class="search">
        <label for="log-search" class="sr-only">{t("diagnostics.search")}</label>
        <input id="log-search" type="search" bind:value={query} placeholder={t("diagnostics.search")} />
      </div>
      <label class="follow"><input type="checkbox" bind:checked={follow} />{t("diagnostics.follow")}</label>
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (región con scroll: tiene que poder enfocarse con el teclado) -->
    <div class="viewer" bind:this={viewer} role="log" aria-label={t("diagnostics.log")} tabindex="0">
      {#each filtered as l, i (i)}
        <div class="line" data-level={l.level}>
          <time class="tabular">{formatTime(l.at_ms ?? 0, i18n.locale)}</time>
          <span class="lv">{t(`log_level.${l.level}`)}</span>
          <span class="tg">{l.target}</span>
          <span class="msg">{l.message}</span>
        </div>
      {:else}
        <p class="muted">{t("diagnostics.log_empty")}</p>
      {/each}
    </div>
    <p class="muted small">{t("diagnostics.no_secrets")}</p>
  </section>
</div>

<style>
  .page {
    display: grid;
    gap: 1.5rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 1240px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 1rem;
  }
  .ok {
    display: flex;
    gap: 0.4rem;
    color: var(--ok);
    font-weight: 700;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 380px), 1fr));
    gap: 1rem;
  }
  .pad {
    padding: 1rem 1.1rem;
  }
  h2 {
    font-size: 1.25rem;
    margin-bottom: 0.7rem;
  }
  .table {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.92rem;
  }
  th,
  td {
    text-align: left;
    padding: 0.7rem 0.9rem;
    vertical-align: top;
  }
  thead th {
    font: 600 0.8rem var(--font-body);
    color: var(--muted);
    border-bottom: 1px solid var(--line);
  }
  tbody tr + tr {
    border-top: 1px solid var(--line);
  }
  .small {
    font-size: 0.8rem;
  }
  .logs {
    display: grid;
    gap: 0.6rem;
  }
  .loghead {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1rem;
  }
  .loghead h2 {
    margin: 0;
  }
  .levels {
    display: flex;
    gap: 0.6rem;
    border: 0;
    padding: 0;
    margin: 0;
  }
  .lvl,
  .follow {
    display: flex;
    gap: 0.35rem;
    align-items: center;
    font-size: 0.88rem;
  }
  .search {
    flex: 1 1 220px;
  }
  .viewer {
    height: 320px;
    overflow: auto;
    padding: 0.8rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--line);
    background: var(--bg-deep);
    font: 0.82rem/1.6 var(--font-mono);
  }
  .line {
    display: grid;
    grid-template-columns: 6.5em 4.5em 9em 1fr;
    gap: 0.6rem;
  }
  .lv {
    text-transform: uppercase;
    font-size: 0.75rem;
  }
  .line[data-level="error"] .lv {
    color: var(--danger);
  }
  .line[data-level="warn"] .lv {
    color: var(--warn);
  }
  .line[data-level="info"] .lv {
    color: var(--ok);
  }
  .line[data-level="debug"] {
    color: var(--muted);
  }
  .tg {
    color: var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  time {
    color: var(--muted);
  }
</style>
