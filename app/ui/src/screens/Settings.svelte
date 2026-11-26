<script lang="ts">
  // D. Ajustes generados desde el catálogo que devuelve `settings_get`: nada de esta pantalla
  // conoce un ajuste concreto. Niveles Básico/Avanzado/Experto, ámbito global/foco/perfil,
  // restaurar, vista previa en vivo y perfiles (crear, duplicar, renombrar, atajo, eliminar).
  import { tick } from "svelte";
  import type { Profile_Serialize, SettingDef, SettingLevel, SettingScope, SettingValue } from "../lib/bindings";
  import { activateProfile, app, deviceName, raise, refreshSettings } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { route } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { LEVELS, groupByLevel, limitsFor, overridesFor, shadowedBy, valueIn, visibleDefs } from "../lib/settings";
  import Eclipse from "../lib/components/Eclipse.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import MoreMenu from "../lib/components/MoreMenu.svelte";
  import SettingControl from "../lib/components/SettingControl.svelte";
  import UpdatesPanel from "../lib/components/UpdatesPanel.svelte";

  const LEVEL_KEY = "openlight.settings.level";
  function savedLevel(): SettingLevel {
    try {
      const v = localStorage.getItem(LEVEL_KEY);
      if (v === "basic" || v === "advanced" || v === "expert") return v;
    } catch {
      // comodidad: si no se puede leer, arranca en Básico
    }
    return "basic";
  }
  let level = $state<SettingLevel>(savedLevel());
  $effect(() => {
    try {
      localStorage.setItem(LEVEL_KEY, level);
    } catch {
      // no pasa nada
    }
  });

  // Ámbito: "global", "device:<id>" o "profile:<id>". Desde el panel se llega con el foco elegido.
  let scopeKey = $state(route.param && app.devices.some((d) => d.id === route.param) ? `device:${route.param}` : "global");
  const scope = $derived<SettingScope>(scopeKey === "global" ? { kind: "global" } : scopeKey.startsWith("device:") ? { kind: "device", id: scopeKey.slice(7) } : { kind: "profile", id: scopeKey.slice(8) });

  $effect(() => {
    if (!app.settings) void refreshSettings();
  });

  const snapshot = $derived(app.settings);
  const groups = $derived(snapshot ? groupByLevel(visibleDefs(snapshot.catalog, level)) : []);
  const own = $derived(snapshot ? overridesFor(snapshot, scope, app.profiles) : {});
  const hidden = $derived(snapshot ? snapshot.catalog.length - visibleDefs(snapshot.catalog, level).length : 0);

  // En «Todos los focos»: los que tienen un valor propio y por eso no cambian con este.
  function overriding(def: SettingDef) {
    if (!snapshot || scope.kind !== "global") return [];
    return snapshot.devices.flatMap(({ device, overrides }) => {
      const value = overrides[def.id];
      return value === undefined ? [] : [{ device, name: deviceName(device), value }];
    });
  }

  // En un foco: si el valor es propio y cuál usaría sin él (el de todos, o el del perfil activo).
  function source(def: SettingDef) {
    if (!snapshot || scope.kind !== "device") return undefined;
    const active = app.profiles.find((p) => p.id === snapshot.active_profile);
    const profile = active?.values[def.id] !== undefined ? profileName(active) : undefined;
    return { own: own[def.id] !== undefined, common: snapshot.effective[def.id] as SettingValue, profile };
  }

  // Dónde no se va a notar lo que se cambie en este ámbito (en «Todos los focos», los focos con
  // valor propio se muestran aparte, con su acción).
  function note(def: SettingDef): string | undefined {
    if (!snapshot) return undefined;
    const hidden = shadowedBy(snapshot, scope, def.id, app.profiles);
    const profile = app.profiles.find((p) => p.id === hidden.profile);
    const lines = [
      profile ? t("settings.shadowed_profile", { name: profileName(profile) }) : "",
      scope.kind !== "global" && hidden.devices.length ? t("settings.shadowed_devices", { names: hidden.devices.map(deviceName).join(", ") }) : "",
    ];
    return lines.filter(Boolean).join(" ") || undefined;
  }

  async function set(def: SettingDef, value: SettingValue) {
    const r = await attempt(() => commands.settingsSet(scope, def.id, value));
    if (r.ok) {
      app.settings = r.value;
      if (scope.kind === "profile") await refreshProfiles();
      syncDevice();
      // Un valor rechazado no se arregla reintentando: "Reintentar" solo descarta el aviso.
    } else raise(r.error, r.error.code === "invalid_setting" ? undefined : () => set(def, value));
  }

  async function reset(def: SettingDef | null, target: SettingScope = scope) {
    const r = await attempt(() => commands.settingsReset(target, def?.id ?? null));
    if (r.ok) {
      app.settings = r.value;
      if (target.kind === "profile") await refreshProfiles();
      syncDevice();
    } else raise(r.error);
  }

  function syncDevice() {
    for (const d of app.devices) d.overrides = app.settings?.devices.find((o) => o.device === d.id)?.overrides ?? {};
  }

  async function refreshProfiles() {
    const r = await attempt(() => commands.profilesList());
    if (r.ok) app.profiles = r.value;
    else raise(r.error, refreshProfiles);
  }

  // ---------- Perfiles ----------

  let newName = $state("");
  let renaming = $state<string | null>(null);
  let renameValue = $state("");
  let capturing = $state<string | null>(null);
  let deleting = $state<Profile_Serialize | null>(null);
  let dialog: HTMLDialogElement | undefined = $state();
  let profilesTitle: HTMLElement | undefined = $state();
  let scopeSelect: HTMLSelectElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (deleting && !dialog.open) dialog.showModal();
    if (!deleting && dialog.open) dialog.close();
  });
  const profileName = (p: Profile_Serialize) => (p.builtin ? t(p.name) : p.name);

  async function createProfile(from: string | null) {
    const base = from ? app.profiles.find((p) => p.id === from) : null;
    const name = from ? t("settings.profiles.copy_of", { name: base ? profileName(base) : "" }) : newName.trim();
    if (!name) return;
    const r = await attempt(() => commands.profilesCreate(name, from));
    if (!r.ok) return raise(r.error);
    newName = "";
    await refreshProfiles();
    scopeKey = `profile:${r.value.id}`;
  }

  async function update(p: Profile_Serialize, patch: Partial<Profile_Serialize>) {
    const r = await attempt(() => commands.profilesUpdate({ ...$state.snapshot(p), ...patch }));
    if (!r.ok) return raise(r.error);
    await refreshProfiles();
  }

  async function remove() {
    const p = deleting;
    deleting = null;
    if (!p) return;
    const r = await attempt(() => commands.profilesDelete(p.id));
    if (!r.ok) return raise(r.error);
    if (scopeKey === `profile:${p.id}`) scopeKey = "global";
    await Promise.all([refreshProfiles(), refreshSettings()]);
    // La tarjeta que tenía el foco ya no está: queda en el título de la lista.
    await tick();
    profilesTitle?.focus();
  }

  function startRename(p: Profile_Serialize) {
    renaming = p.id;
    renameValue = p.name;
  }

  /** Cierra el renombrado (guardando o no) y devuelve el foco al menú del perfil. */
  async function endRename(p: Profile_Serialize, save: boolean) {
    renaming = null;
    if (save) void update(p, { name: renameValue.trim() || p.name });
    await tick();
    document.querySelector<HTMLElement>(`[data-profile="${CSS.escape(p.id)}"] [aria-haspopup="menu"]`)?.focus();
  }

  const selectAll = (input: HTMLInputElement) => input.select();

  /** El selector de ámbito puede quedar lejos de la lista: el foco va ahí y se ve qué se edita. */
  async function edit(p: Profile_Serialize) {
    scopeKey = `profile:${p.id}`;
    await tick();
    scopeSelect?.focus();
  }

  function captureKey(e: KeyboardEvent, p: Profile_Serialize) {
    e.preventDefault();
    if (e.key === "Escape") return (capturing = null);
    if (e.key === "Backspace" || e.key === "Delete") {
      capturing = null;
      return update(p, { shortcut: null });
    }
    if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
    const mods = [(e.metaKey || e.ctrlKey) && "CmdOrCtrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
    if (!mods.length) return;
    const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
    capturing = null;
    void update(p, { shortcut: [...mods, key].join("+") });
  }

  const preview = $derived(Object.values(app.live).find((l) => scope.kind !== "device" || l.device === scope.id));
</script>

<div class="page">
  <header class="head">
    <div>
      <span class="eyebrow">{t("nav.settings")}</span>
      <h1>{t("settings.title")}</h1>
    </div>
    <div class="preview card" aria-live="off">
      <Eclipse screen={preview?.screen} sent={preview?.sent} off={app.session !== "active"} size={64} />
      <div>
        <p class="h">{t("settings.preview")}</p>
        <p class="muted small">{app.session === "active" ? t("settings.preview_on") : t("settings.preview_off")}</p>
      </div>
    </div>
  </header>

  <div class="bar">
    <div class="field scope">
      <label for="scope">{t("settings.scope")}</label>
      <select id="scope" bind:this={scopeSelect} bind:value={scopeKey}>
        <option value="global">{t("settings.scope_global")}</option>
        <optgroup label={t("settings.scope_devices")}>
          {#each app.devices as d (d.id)}<option value="device:{d.id}">{d.name}</option>{/each}
        </optgroup>
        <optgroup label={t("settings.scope_profiles")}>
          {#each app.profiles as p (p.id)}<option value="profile:{p.id}">{profileName(p)}</option>{/each}
        </optgroup>
      </select>
    </div>
    <div class="levels" role="radiogroup" aria-label={t("settings.level_label")}>
      {#each LEVELS as l (l)}
        <label class:on={level === l}>
          <input type="radio" name="level" value={l} bind:group={level} />
          {t(`settings.level.${l}`)}
        </label>
      {/each}
    </div>
    <button type="button" class="btn ghost small" onclick={() => reset(null)}><Icon name="refresh" size={15} />{t("settings.restore_all")}</button>
  </div>

  <p class="scope-note muted">{t(`settings.scope_note.${scope.kind}`)}</p>

  <div class="layout">
    <div class="controls">
      {#if !snapshot}
        <p role="status" class="muted"><span class="spinner" aria-hidden="true"></span> {t("common.loading")}</p>
      {:else}
        {#each groups as g (g.level)}
          <section aria-labelledby="lvl-{g.level}">
            <h2 id="lvl-{g.level}" class="lvl">{t(`settings.level.${g.level}`)} <span class="muted">· {g.defs.length}</span></h2>
            <div class="list">
              {#each g.defs as def (def.id)}
                {@const v = valueIn(snapshot, scope, def.id, app.profiles)}
                <SettingControl {def} value={v.value} inherited={v.inherited} canReset={scope.kind === "global" ? undefined : own[def.id] !== undefined} {...limitsFor(snapshot, scope, def.id, app.profiles)} note={note(def)} overrides={overriding(def)} source={source(def)} onchange={(val) => set(def, val)} onreset={() => reset(def)} onusecommon={(id) => reset(def, { kind: "device", id })} />
              {/each}
            </div>
          </section>
        {/each}
        {#if hidden > 0}
          <p class="more muted">{t("settings.more_hidden", { n: hidden })}</p>
        {/if}
      {/if}
    </div>

    <aside class="profiles card" aria-labelledby="profiles-title">
      <h2 id="profiles-title" bind:this={profilesTitle} tabindex="-1">{t("settings.profiles.title")}</h2>
      <p class="muted small">{t("settings.profiles.help")}</p>
      <ul>
        {#each app.profiles as p (p.id)}
          {@const active = snapshot?.active_profile === p.id}
          <li class:active data-profile={p.id}>
            {#if renaming === p.id}
              <form class="rename" onsubmit={(e) => { e.preventDefault(); void endRename(p, true); }}>
                <label class="sr-only" for="rename-{p.id}">{t("settings.profiles.rename")}</label>
                <input id="rename-{p.id}" type="text" bind:value={renameValue} {@attach selectAll} onkeydown={(e) => e.key === "Escape" && void endRename(p, false)} />
                <button type="submit" class="btn small primary">{t("common.save")}</button>
              </form>
            {:else}
              <div class="pname">
                <span class="n">{profileName(p)}</span>
                {#if p.builtin}<span class="chip">{t("settings.profiles.builtin")}</span>{/if}
                {#if active}<span class="chip ok badge"><Icon name="check" size={13} />{t("settings.profiles.active")}</span>{/if}
              </div>
            {/if}
            <div class="pactions" role="group" aria-label={t("settings.profiles.actions_of", { name: profileName(p) })}>
              <!-- Un solo botón que cambia: así el foco no se pierde al activar o desactivar. -->
              <button type="button" class="btn small" class:primary={!active} class:secondary={active} onclick={() => activateProfile(active ? null : p.id)}>
                <Icon name={active ? "stop" : "play"} size={14} />{active ? t("settings.profiles.deactivate") : t("settings.profiles.activate")}
              </button>
              <button type="button" class="btn secondary small" title={t("settings.profiles.edit_help")} onclick={() => edit(p)}><Icon name="sliders" size={14} />{t("settings.profiles.edit")}</button>
              <!-- Los de fábrica solo se pueden duplicar: no hace falta un menú. -->
              {#if p.builtin}
                <button type="button" class="btn secondary small" onclick={() => createProfile(p.id)}><Icon name="copy" size={14} />{t("settings.profiles.duplicate")}</button>
              {:else}
                <MoreMenu
                  text={t("settings.profiles.more")}
                  label={t("settings.profiles.more_named", { name: p.name })}
                  items={[
                    { label: t("settings.profiles.duplicate"), icon: "copy", run: () => createProfile(p.id) },
                    { label: t("settings.profiles.rename"), icon: "pencil", run: () => startRename(p) },
                  ]}
                />
                <button type="button" class="btn destructive small remove" title={t("settings.profiles.delete")} aria-label={t("settings.profiles.delete_named", { name: p.name })} onclick={() => (deleting = p)}><Icon name="trash" size={16} /></button>
              {/if}
            </div>
            <div class="shortcut">
              <span class="muted small">{t("settings.profiles.shortcut")}</span>
              <button
                type="button"
                class="btn secondary small key"
                class:listening={capturing === p.id}
                onclick={() => (capturing = capturing === p.id ? null : p.id)}
                onkeydown={(e) => capturing === p.id && captureKey(e, p)}
                onblur={() => capturing === p.id && (capturing = null)}
              >
                {#if capturing === p.id}{t("settings.profiles.press_keys")}{:else if p.shortcut}<kbd>{p.shortcut}</kbd>{:else}{t("settings.profiles.assign")}{/if}<span class="sr-only"> · {t("settings.profiles.shortcut_of", { name: profileName(p) })}</span>
              </button>
            </div>
          </li>
        {/each}
      </ul>
      <form class="new" onsubmit={(e) => { e.preventDefault(); void createProfile(null); }}>
        <label for="new-profile" class="label">{t("settings.profiles.new")}</label>
        <div class="row">
          <input id="new-profile" type="text" bind:value={newName} placeholder={t("settings.profiles.new_placeholder")} />
          <button type="submit" class="btn primary small" disabled={!newName.trim()}><Icon name="plus" size={15} />{t("settings.profiles.create")}</button>
        </div>
      </form>
    </aside>
  </div>

  <UpdatesPanel />
</div>

<dialog bind:this={dialog} class="confirm card" aria-labelledby="del-title" aria-describedby="del-text" onclose={() => (deleting = null)}>
  {#if deleting}
    <h2 id="del-title">{t("settings.profiles.delete_title", { name: deleting.name })}</h2>
    <p id="del-text" class="muted">{t("settings.profiles.delete_text")}</p>
    <div class="answers">
      <button type="button" class="btn secondary" onclick={() => (deleting = null)}>{t("common.cancel")}</button>
      <button type="button" class="btn destructive solid" onclick={remove}><Icon name="trash" size={15} />{t("settings.profiles.delete_confirm")}</button>
    </div>
  {/if}
</dialog>

<style>
  .page {
    display: grid;
    gap: 1.2rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 1240px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 1.5rem;
    flex-wrap: wrap;
  }
  .preview {
    display: flex;
    gap: 1rem;
    align-items: center;
    padding: 0.8rem 1.1rem 0.8rem 0.9rem;
  }
  .h {
    font-weight: 700;
  }
  .small {
    font-size: 0.84rem;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    align-items: end;
    padding: 1rem 0;
    border-block: 1px solid var(--line);
  }
  .scope {
    min-width: 260px;
  }
  .levels {
    display: flex;
    padding: 4px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .levels label {
    position: relative;
    padding: 0.45em 1.1em;
    border-radius: 999px;
    font-weight: 700;
    cursor: pointer;
    color: var(--muted);
  }
  .levels input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: pointer;
  }
  .levels label.on {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .levels label:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .scope-note {
    font-size: 0.9rem;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 390px;
    gap: 1.5rem;
    align-items: start;
  }
  .controls {
    display: grid;
    gap: 1.6rem;
  }
  .lvl {
    font-size: 1.1rem;
    margin-bottom: 0.7rem;
  }
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 330px), 1fr));
    gap: 0.8rem;
  }
  .more {
    font-size: 0.9rem;
  }
  .profiles {
    position: sticky;
    top: 1.2rem;
    display: grid;
    gap: 0.8rem;
    padding: 1.1rem;
  }
  .profiles ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .profiles li {
    display: grid;
    gap: 0.5rem;
    padding: 0.8rem;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .profiles li.active {
    box-shadow: inset 0 0 0 2px var(--ok);
  }
  .pname {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .n {
    font-weight: 700;
    overflow-wrap: anywhere;
  }
  /* La tarjeta ya es gris: la etiqueta necesita fondo propio. */
  .pname .chip:not(.ok) {
    background: var(--surface);
  }
  .badge {
    margin-left: auto;
  }
  .pactions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: center;
  }
  .pactions > .btn {
    padding-inline: 0.8em;
  }
  /* Eliminar va apartado del resto. */
  .pactions > .remove {
    width: 36px;
    margin-left: auto;
    padding: 0;
  }
  .shortcut {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    justify-content: space-between;
  }
  .key.listening {
    box-shadow: inset 0 0 0 2px var(--accent);
    animation: pulse 1s infinite;
  }
  .rename,
  .row {
    display: flex;
    gap: 0.4rem;
  }
  .confirm {
    max-width: 440px;
    padding: 1.4rem;
    color: var(--text);
  }
  .confirm::backdrop {
    background: rgb(8 6 4 / 0.55);
  }
  .confirm h2 {
    overflow-wrap: anywhere;
  }
  .confirm p {
    margin: 0.6rem 0 1.2rem;
  }
  .answers {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .new {
    display: grid;
    gap: 0.4rem;
    padding-top: 0.6rem;
    border-top: 1px solid var(--line);
  }
  @media (max-width: 1050px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .profiles {
      position: static;
    }
  }
</style>
