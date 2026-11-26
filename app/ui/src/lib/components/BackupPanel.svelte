<script lang="ts">
  // Respaldo sin claves: exportar lo configurado, importarlo (resumen primero) y bajar de la nube
  // las claves que quedan faltando. El archivo lo elige el usuario: al exportar, en un diálogo que
  // abre el backend; al importar, con el selector del sistema (la ventana solo le pasa el texto).
  import { onMount } from "svelte";
  import type { BackupSummary, DuplicatePolicy, ImportReport, MissingKeysReport, UserFacingError } from "../bindings";
  import { app, deviceName, raise, refreshDevices, refreshKeys, refreshOnboarding, refreshSettings } from "../app.svelte";
  import { attempt, commands } from "../ipc";
  import { go } from "../router.svelte";
  import { i18n, t } from "../i18n/index.svelte";
  import ErrorCard from "./ErrorCard.svelte";
  import Icon from "./Icon.svelte";

  /** El mismo tope que el backend: no vale la pena ni leer algo más grande. */
  const MAX_BYTES = 2 * 1024 * 1024;
  const POLICIES: DuplicatePolicy[] = ["skip", "replace"];

  let busy = $state<"export" | "read" | "import" | "keys" | null>(null);
  let error = $state<{ at: "export" | "import" | "keys"; value: UserFacingError } | null>(null);
  let savedTo = $state<string | null>(null);
  let pending = $state<{ content: string; summary: BackupSummary } | null>(null);
  let policy = $state<DuplicatePolicy>("skip");
  let report = $state<ImportReport | null>(null);
  let keys = $state<MissingKeysReport | null>(null);
  let input: HTMLInputElement | undefined = $state();
  let dialog: HTMLDialogElement | undefined = $state();

  onMount(() => void refreshKeys());

  $effect(() => {
    if (!dialog) return;
    if (pending && !dialog.open) dialog.showModal();
    if (!pending && dialog.open) dialog.close();
  });

  // Un foco que se quitó después deja de contar
  const missing = $derived(app.withoutKey.filter((id) => app.devices.some((d) => d.id === id)));
  const names = (ids: string[]) => ids.map(deviceName).join(", ");

  function exportedOn(iso: string): string {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : new Intl.DateTimeFormat(i18n.locale, { dateStyle: "long" }).format(date);
  }

  async function save() {
    busy = "export";
    error = null;
    savedTo = null;
    const r = await attempt(() => commands.backupExport());
    busy = null;
    if (!r.ok) error = { at: "export", value: r.error };
    // `null`: se cerró el diálogo sin guardar
    else savedTo = r.value;
  }

  function choose() {
    if (!input) return;
    // Vacío, así elegir de nuevo el mismo archivo también avisa
    input.value = "";
    input.click();
  }

  async function picked() {
    const file = input?.files?.[0];
    if (!file) return;
    error = null;
    report = null;
    keys = null;
    busy = "read";
    const r = await attempt(async () => {
      // Algo demasiado grande ni se lee: se rechaza como cualquier archivo que no sirve
      const content = file.size > MAX_BYTES ? "" : await file.text();
      return { content, summary: await commands.backupInspect(content) };
    });
    busy = null;
    if (!r.ok) {
      error = { at: "import", value: r.error };
      return;
    }
    policy = "skip";
    pending = r.value;
  }

  async function apply() {
    const chosen = pending;
    pending = null;
    if (!chosen) return;
    busy = "import";
    const r = await attempt(() => commands.backupImport(chosen.content, policy));
    busy = null;
    if (!r.ok) {
      error = { at: "import", value: r.error };
      return;
    }
    report = r.value;
    await Promise.all([refreshDevices(), refreshSettings(), refreshKeys()]);
  }

  async function fetchKeys() {
    busy = "keys";
    error = null;
    keys = null;
    const r = await attempt(() => commands.fetchMissingKeys());
    busy = null;
    if (!r.ok) {
      error = { at: "keys", value: r.error };
      return;
    }
    keys = r.value;
    await Promise.all([refreshDevices(), refreshKeys()]);
  }

  /** Al paso del asistente donde se cargan el Access ID y el Access Secret. */
  async function addCredentials() {
    const later = ["credentials", "choose_devices", "test_devices", "zones", "calibrate", "done"];
    const done = (app.onboarding?.completed ?? []).filter((s) => !later.includes(s));
    const r = await attempt(() => commands.onboardingSave("credentials", done));
    if (!r.ok) return raise(r.error);
    await refreshOnboarding();
    go("onboarding");
  }
</script>

<section class="backup card" aria-labelledby="backup-title">
  <div class="intro">
    <h2 id="backup-title">{t("backup.title")}</h2>
    <p class="muted">{t("backup.lead")}</p>
    <p class="muted notice"><Icon name="key" size={15} /><span>{t("backup.no_keys")}</span></p>
  </div>
  <div class="row">
    <button type="button" class="btn" onclick={save} disabled={busy !== null || !app.devices.length}>
      {#if busy === "export"}<span class="spinner" aria-hidden="true"></span>{/if}{t("backup.export")}
    </button>
    <button type="button" class="btn" onclick={choose} disabled={busy !== null}>
      {#if busy === "read" || busy === "import"}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="folder" size={15} />{/if}{t("backup.import")}
    </button>
    <input bind:this={input} type="file" accept=".json,application/json" onchange={picked} hidden />
  </div>

  {#if savedTo}<p class="ok" role="status"><Icon name="check" size={16} /><span class="path">{t("backup.saved", { path: savedTo })}</span></p>{/if}
  {#if error && error.at !== "keys"}
    <ErrorCard error={error.value} retry={error.at === "export" ? save : choose} retryLabel={error.at === "import" ? t("backup.pick_again") : undefined} />
  {/if}
  {#if report}
    <div class="report" role="status">
      <p class="ok"><Icon name="check" size={16} />{t("backup.done", { devices: report.devices.length, calibrations: report.calibrations })}</p>
      <p class="muted">{t(report.settings_applied ? "backup.settings_applied" : "backup.settings_kept")}</p>
      {#each report.warnings as w (w)}<p class="muted">{t(w)}</p>{/each}
    </div>
  {/if}

  {#if keys?.fetched.length}<p class="ok" role="status"><Icon name="check" size={16} />{t("backup.keys_fetched", { n: keys.fetched.length })}</p>{/if}
  {#if missing.length}
    <div class="keys">
      <h3>{t("backup.keys_title")}</h3>
      <p>{t("backup.keys_text", { names: names(missing) })}</p>
      {#if keys?.not_found.length}<p class="muted" role="status">{t("backup.keys_not_found", { names: names(keys.not_found) })}</p>{/if}
      {#if error?.at === "keys"}<ErrorCard error={error.value} retry={fetchKeys} />{/if}
      <div class="row">
        <button type="button" class="btn primary" onclick={fetchKeys} disabled={busy !== null}>
          {#if busy === "keys"}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="key" size={15} />{/if}{t("backup.keys_fetch")}
        </button>
        {#if error?.at === "keys" && error.value.code === "cloud_no_credentials"}
          <button type="button" class="btn" onclick={addCredentials}>{t("backup.keys_credentials")}</button>
        {/if}
      </div>
    </div>
  {/if}
</section>

<dialog bind:this={dialog} class="summary card" aria-labelledby="backup-summary-title" onclose={() => (pending = null)}>
  {#if pending}
    {@const s = pending.summary}
    <h2 id="backup-summary-title">{t("backup.summary_title")}</h2>
    <p class="muted">{t("backup.made", { date: exportedOn(s.created_at), version: s.app_version })}</p>
    {#each [["new", s.new_devices], ["existing", s.existing_devices]] as const as [kind, list] (kind)}
      <div>
        <p class="count">{t(`backup.${kind}`, { n: list.length })}</p>
        {#if list.length}<p class="muted names">{list.map((d) => d.name).join(", ")}</p>{/if}
      </div>
    {/each}
    <fieldset>
      <legend>{t("backup.policy")}</legend>
      {#each POLICIES as p (p)}
        <label>
          <input type="radio" bind:group={policy} value={p} />
          <span><strong>{t(`backup.${p}`)}</strong><span class="muted help">{t(`backup.${p}_help`)}</span></span>
        </label>
      {/each}
    </fieldset>
    {#if s.new_devices.length}<p class="muted notice"><Icon name="key" size={15} /><span>{t("backup.keys_later")}</span></p>{/if}
    <div class="row end">
      <button type="button" class="btn" onclick={() => (pending = null)}>{t("common.cancel")}</button>
      <button type="button" class="btn primary" onclick={apply}>{t("backup.confirm")}</button>
    </div>
  {/if}
</dialog>

<style>
  .backup {
    display: grid;
    gap: 0.9rem;
    padding: 1.2rem;
  }
  .intro {
    display: grid;
    gap: 0.4rem;
    max-width: 70ch;
  }
  .notice {
    display: flex;
    gap: 0.5rem;
    align-items: start;
  }
  .notice :global(svg) {
    flex: none;
    margin-top: 0.2rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .end {
    justify-content: flex-end;
  }
  .ok {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    color: var(--ok);
    font-weight: 700;
  }
  .path,
  .names {
    overflow-wrap: anywhere;
  }
  .report {
    display: grid;
    gap: 0.3rem;
  }
  .keys {
    display: grid;
    gap: 0.6rem;
    padding-top: 0.9rem;
    border-top: 1px solid var(--line);
  }
  .keys h3 {
    font-size: 1rem;
  }
  .summary {
    width: min(480px, calc(100vw - 2rem));
    padding: 1.4rem;
    color: var(--text);
  }
  .summary[open] {
    display: grid;
    gap: 0.9rem;
  }
  .summary::backdrop {
    background: rgb(8 6 4 / 0.55);
  }
  .count {
    font-weight: 700;
  }
  fieldset {
    display: grid;
    gap: 0.6rem;
    border: 0;
    padding: 0;
    margin: 0;
  }
  legend {
    font-weight: 700;
    margin-bottom: 0.5rem;
  }
  fieldset label {
    display: flex;
    gap: 0.6rem;
    align-items: start;
  }
  .help {
    display: block;
  }
</style>
