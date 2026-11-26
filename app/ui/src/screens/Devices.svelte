<script lang="ts">
  // F. Dispositivos: agregar o quitar focos, "Renovar clave" en un clic, IP fija o automática,
  // protocolo y DPs. Abajo, las zonas de cada foco.
  import type { ConfiguredDevice_Serialize, UserFacingError } from "../lib/bindings";
  import { app, dismissCode, keyArrived, raise, refreshDevices } from "../lib/app.svelte";
  import { attempt, commands } from "../lib/ipc";
  import { go } from "../lib/router.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import BackupPanel from "../lib/components/BackupPanel.svelte";
  import ErrorCard from "../lib/components/ErrorCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import ZonesPanel from "../lib/components/ZonesPanel.svelte";

  let editing = $state<Record<string, { name: string; auto: boolean; ip: string }>>({});
  let errors = $state<Record<string, UserFacingError | null>>({});
  let renewed = $state<Record<string, boolean>>({});
  let confirmRemove = $state<ConfiguredDevice_Serialize | null>(null);
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (confirmRemove && !dialog.open) dialog.showModal();
    if (!confirmRemove && dialog.open) dialog.close();
  });

  function edit(d: ConfiguredDevice_Serialize) {
    editing[d.id] = { name: d.name, auto: d.auto_ip, ip: d.ip ?? "" };
  }

  async function save(d: ConfiguredDevice_Serialize) {
    const e = editing[d.id];
    const r = await attempt(() => commands.deviceUpdate({ id: d.id, name: e.name, ip: e.auto ? null : e.ip.trim() }));
    if (!r.ok) return (errors[d.id] = r.error);
    errors[d.id] = null;
    delete editing[d.id];
    await refreshDevices();
  }

  async function renew(d: ConfiguredDevice_Serialize) {
    app.busy[`renew:${d.id}`] = true;
    renewed[d.id] = false;
    const r = await attempt(() => commands.renewKey(d.id));
    app.busy[`renew:${d.id}`] = false;
    if (!r.ok) return (errors[d.id] = r.error);
    errors[d.id] = null;
    renewed[d.id] = true;
    dismissCode("device_key_rejected", d.id);
    keyArrived(d.id);
  }

  async function remove() {
    const d = confirmRemove;
    confirmRemove = null;
    if (!d) return;
    const r = await attempt(() => commands.deviceRemove(d.id));
    if (!r.ok) return raise(r.error);
    await refreshDevices();
  }

  async function addMore() {
    const done = app.onboarding?.completed ?? [];
    await attempt(() => commands.onboardingSave("choose_devices", done.filter((s) => !["choose_devices", "test_devices", "zones", "calibrate", "done"].includes(s))));
    const r = await attempt(() => commands.onboardingState());
    if (r.ok) app.onboarding = r.value;
    go("onboarding");
  }

  function dps(d: ConfiguredDevice_Serialize): [string, string][] {
    const c = d.capabilities;
    const rows: [string, string | null][] = [
      ["switch", c.dp_switch],
      ["mode", c.dp_mode],
      ["bright", c.dp_bright ? `${c.dp_bright.dp} (${c.dp_bright.min}–${c.dp_bright.max})` : null],
      ["temp", c.dp_temp ? `${c.dp_temp.dp} (${c.dp_temp.min}–${c.dp_temp.max})` : null],
      ["colour", c.dp_colour ? `${c.dp_colour[0]} · ${c.dp_colour[1]}` : null],
      ["control", c.dp_control],
    ];
    return rows.filter((r): r is [string, string] => r[1] !== null);
  }

  const stateOf = (id: string) => app.health[id]?.state;
</script>

<div class="page">
  <header class="head">
    <div>
      <span class="eyebrow">{t("nav.devices")}</span>
      <h1>{t("devices.title")}</h1>
    </div>
    <button type="button" class="btn primary" onclick={addMore}><Icon name="plus" size={16} />{t("devices.add")}</button>
  </header>

  <ul class="list">
    {#each app.devices as d (d.id)}
      {@const e = editing[d.id]}
      {@const st = stateOf(d.id)}
      <li class="card device">
        <div class="top">
          <div class="id">
            {#if e}
              <label class="sr-only" for="name-{d.id}">{t("devices.name")}</label>
              <input id="name-{d.id}" type="text" bind:value={e.name} />
            {:else}
              <h2>{d.name}</h2>
            {/if}
            <p class="mono muted small">{d.id}</p>
          </div>
          <div class="chips">
            <span class="chip">v{d.version}</span>
            <span class="chip" class:ok={d.calibrated}>{d.calibrated ? t("devices.calibrated") : t("devices.not_calibrated")}</span>
            {#if app.withoutKey.includes(d.id)}<span class="chip warn">{t("devices.no_key")}</span>{/if}
            {#if st?.kind === "error"}<span class="chip danger">{t(`error.${st.code}.title`)}</span>{/if}
          </div>
        </div>

        <div class="grid">
          <section aria-labelledby="ip-{d.id}">
            <h3 id="ip-{d.id}">{t("devices.ip")}</h3>
            {#if e}
              <fieldset class="ipmode">
                <legend class="sr-only">{t("devices.ip_mode")}</legend>
                <label><input type="radio" bind:group={e.auto} value={true} /> {t("devices.ip_auto")}</label>
                <label><input type="radio" bind:group={e.auto} value={false} /> {t("devices.ip_fixed")}</label>
              </fieldset>
              {#if !e.auto}
                <label class="sr-only" for="ipv-{d.id}">{t("devices.ip_fixed")}</label>
                <input id="ipv-{d.id}" type="text" class="mono" bind:value={e.ip} placeholder="192.168.1.42" inputmode="decimal" />
              {/if}
            {:else}
              <p><span class="mono">{d.ip ?? "—"}</span> <span class="muted">· {d.auto_ip ? t("devices.ip_auto") : t("devices.ip_fixed")}</span></p>
            {/if}
          </section>
          <section aria-labelledby="dps-{d.id}">
            <h3 id="dps-{d.id}">{t("devices.dps")}</h3>
            <table class="dps">
              <caption class="sr-only">{t("devices.dps_caption", { name: d.name })}</caption>
              <tbody>
                {#each dps(d) as [k, v] (k)}<tr><th scope="row">{t(`devices.dp.${k}`)}</th><td class="mono">{v}</td></tr>{/each}
                <tr><th scope="row">{t("devices.dp.modes")}</th><td>{d.capabilities.modes.join(", ")}</td></tr>
              </tbody>
            </table>
          </section>
        </div>

        {#if errors[d.id]}<ErrorCard error={errors[d.id]!} retry={() => renew(d)} />{/if}
        {#if renewed[d.id]}<p class="ok" role="status"><Icon name="check" size={16} />{t("devices.renewed")}</p>{/if}

        <div class="actions">
          <button type="button" class="btn small" class:primary={st?.kind === "error" && st.code === "device_key_rejected"} disabled={app.busy[`renew:${d.id}`]} onclick={() => renew(d)}>
            {#if app.busy[`renew:${d.id}`]}<span class="spinner" aria-hidden="true"></span>{:else}<Icon name="key" size={15} />{/if}{t("fix.renew_key")}
          </button>
          {#if e}
            <button type="button" class="btn small primary" onclick={() => save(d)}>{t("common.save")}</button>
            <button type="button" class="btn small ghost" onclick={() => delete editing[d.id]}>{t("common.cancel")}</button>
          {:else}
            <button type="button" class="btn small" onclick={() => edit(d)}><Icon name="sliders" size={15} />{t("devices.edit")}</button>
          {/if}
          <button type="button" class="btn small" onclick={() => go("calibrate", d.id)}><Icon name="palette" size={15} />{t("dashboard.calibrate")}</button>
          <button type="button" class="btn small" onclick={() => commands.deviceTest(d.id, "blink").catch(raise)}><Icon name="bulb" size={15} />{t("devices.identify")}</button>
          <span class="spacer"></span>
          <button type="button" class="btn small ghost danger" onclick={() => (confirmRemove = d)}><Icon name="trash" size={15} />{t("devices.remove")}</button>
        </div>
      </li>
    {:else}
      <li class="card empty"><p>{t("dashboard.empty_text")}</p></li>
    {/each}
  </ul>

  <BackupPanel />

  {#if app.devices.length}
    <section class="zones" aria-labelledby="zones-title">
      <h2 id="zones-title">{t("devices.zones")}</h2>
      <p class="muted">{t("onboarding.zones.lead")}</p>
      <ZonesPanel />
    </section>
  {/if}
</div>

<dialog bind:this={dialog} class="confirm card" aria-labelledby="rm-title" onclose={() => (confirmRemove = null)}>
  {#if confirmRemove}
    <h2 id="rm-title">{t("devices.remove_title", { name: confirmRemove.name })}</h2>
    <p class="muted">{t("devices.remove_text")}</p>
    <div class="actions">
      <button type="button" class="btn" onclick={() => (confirmRemove = null)}>{t("common.cancel")}</button>
      <button type="button" class="btn primary" onclick={remove}><Icon name="trash" size={15} />{t("devices.remove")}</button>
    </div>
  {/if}
</dialog>

<style>
  .page {
    display: grid;
    gap: 1.4rem;
    padding: clamp(1.4rem, 3vw, 2.6rem);
    max-width: 1100px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: end;
    gap: 1rem;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1rem;
  }
  .device {
    display: grid;
    gap: 1rem;
    padding: 1.2rem;
  }
  .top {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .id h2 {
    font-size: 1.35rem;
  }
  .small {
    font-size: 0.78rem;
  }
  .chips {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    align-items: start;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1.3fr;
    gap: 1.4rem;
  }
  h3 {
    font: 600 0.85rem var(--font-body);
    color: var(--muted);
    margin-bottom: 0.5rem;
  }
  .ipmode {
    border: 0;
    padding: 0;
    margin: 0 0 0.5rem;
    display: flex;
    gap: 1rem;
  }
  .ipmode label {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .dps {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  .dps th {
    text-align: left;
    font-weight: 400;
    color: var(--muted);
    padding: 0.25rem 0.8rem 0.25rem 0;
  }
  .dps td {
    padding: 0.25rem 0;
  }
  .dps tr + tr {
    border-top: 1px solid var(--line);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    padding-top: 0.8rem;
    border-top: 1px solid var(--line);
  }
  .spacer {
    flex: 1;
  }
  .ok {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    color: var(--ok);
    font-weight: 700;
  }
  .zones {
    display: grid;
    gap: 0.8rem;
  }
  .confirm {
    max-width: 440px;
    padding: 1.4rem;
    color: var(--text);
  }
  .confirm::backdrop {
    background: rgb(8 6 4 / 0.55);
  }
  .confirm p {
    margin: 0.6rem 0 1.2rem;
  }
  .empty {
    padding: 1.4rem;
  }
  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
