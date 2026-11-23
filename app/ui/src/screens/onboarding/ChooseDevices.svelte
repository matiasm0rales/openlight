<script lang="ts">
  // Paso 7: lista de la nube cruzada con el discovery de la red local.
  import type { CloudDeviceInfo_Serialize, DiscoveredDevice, UserFacingError } from "../../lib/bindings";
  import { EVENT_DISCOVERY_FOUND } from "../../lib/bindings";
  import { attempt, commands, on } from "../../lib/ipc";
  import { app, refreshDevices } from "../../lib/app.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import ErrorCard from "../../lib/components/ErrorCard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();

  let loading = $state(true);
  let scanning = $state(false);
  let error = $state<UserFacingError | null>(null);
  let addErrors = $state<UserFacingError[]>([]);
  let found = $state<Record<string, DiscoveredDevice>>({});
  let selected = $state<string[]>(wizard.selected.length ? [...wizard.selected] : app.devices.map((d) => d.id));

  async function load() {
    loading = true;
    error = null;
    const r = await attempt(() => commands.cloudListDevices());
    loading = false;
    if (!r.ok) {
      error = r.error;
      return;
    }
    wizard.cloud = r.value;
    if (!selected.length) selected = r.value.filter((d) => usable(d) && d.lan).map((d) => d.id);
    void scan();
  }

  async function scan() {
    scanning = true;
    const r = await attempt(() => commands.discoverLan(4));
    scanning = false;
    if (r.ok) for (const d of r.value) found[d.id] = d;
    else error = r.error;
  }

  $effect(() => {
    void load();
    const off = on(EVENT_DISCOVERY_FOUND, (d) => {
      found[d.id] = d;
    });
    return () => void off.then((f) => f());
  });

  $effect(() => {
    wizard.selected = [...selected];
    valid = selected.length > 0;
  });

  const usable = (d: CloudDeviceInfo_Serialize) => d.is_light && Boolean(d.capabilities?.dp_colour);
  const lights = $derived(wizard.cloud.filter((d) => d.is_light));
  const others = $derived(wizard.cloud.filter((d) => !d.is_light));
  const lanOf = (d: CloudDeviceInfo_Serialize) => found[d.id] ?? null;

  function caps(d: CloudDeviceInfo_Serialize): string[] {
    const c = d.capabilities;
    if (!c) return [];
    return [c.dp_colour && "colour", c.dp_bright && "white", c.dp_temp && "temperature", c.dp_control && "realtime", c.modes.includes("scene") && "scenes"].filter(Boolean) as string[];
  }

  export async function next(): Promise<boolean> {
    addErrors = [];
    for (const id of selected) {
      if (app.devices.some((d) => d.id === id)) continue;
      const cloud = wizard.cloud.find((d) => d.id === id);
      const r = await attempt(() => commands.deviceAdd({ id, name: cloud?.name ?? id, ip: null }));
      if (!r.ok) addErrors.push(r.error);
    }
    // Los que se destildaron se quitan.
    for (const d of app.devices) {
      if (selected.includes(d.id)) continue;
      const r = await attempt(() => commands.deviceRemove(d.id));
      if (!r.ok) addErrors.push(r.error);
    }
    await refreshDevices();
    return addErrors.length === 0;
  }
</script>

{#if error}<ErrorCard {error} retry={load} />{/if}

{#if loading}
  <p role="status" class="muted"><span class="spinner" aria-hidden="true"></span> {t("onboarding.choose_devices.loading")}</p>
{:else if !error}
  <div class="scan" role="status" aria-live="polite">
    {#if scanning}<span class="radar" aria-hidden="true"></span>{t("onboarding.choose_devices.scanning")}{:else}<Icon name="wifi" size={16} />{t("onboarding.choose_devices.scanned", { n: Object.keys(found).length })}
      <button type="button" class="btn ghost small" onclick={scan}><Icon name="refresh" size={14} />{t("onboarding.choose_devices.rescan")}</button>{/if}
  </div>

  <fieldset class="list">
    <legend class="sr-only">{t("onboarding.choose_devices.legend")}</legend>
    {#each lights as d (d.id)}
      {@const lan = lanOf(d)}
      {@const ok = usable(d)}
      <label class="row card" class:on={selected.includes(d.id)} class:disabled={!ok}>
        <input type="checkbox" value={d.id} bind:group={selected} disabled={!ok} aria-describedby="dev-{d.id}-meta" />
        <span class="bulb" aria-hidden="true"><Icon name="bulb" size={22} /></span>
        <span class="main">
          <span class="name">{d.name}</span>
          <span class="meta" id="dev-{d.id}-meta">
            {#if !ok}<span class="chip danger">{t("onboarding.choose_devices.no_colour")}</span>{/if}
            {#if !d.online}<span class="chip warn">{t("onboarding.choose_devices.offline")}</span>{/if}
            {#if lan}
              <span class="chip ok"><Icon name="wifi" size={13} />{t("onboarding.choose_devices.in_lan")} · <span class="mono">{lan.ip}</span> · v{lan.version}</span>
            {:else if !scanning}
              <span class="chip">{t("onboarding.choose_devices.not_in_lan")}</span>
            {/if}
            {#each caps(d) as c (c)}<span class="chip">{t(`capability.${c}`)}</span>{/each}
          </span>
        </span>
      </label>
    {/each}
  </fieldset>

  {#if others.length}
    <details class="others">
      <summary>{t("onboarding.choose_devices.others", { n: others.length })}</summary>
      <ul>{#each others as d (d.id)}<li>{d.name} <span class="chip">{t("onboarding.choose_devices.not_light")}</span></li>{/each}</ul>
    </details>
  {/if}

  {#each addErrors as e, i (i)}<ErrorCard error={e} />{/each}
{/if}

<style>
  .scan {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    color: var(--muted);
    margin-bottom: 0.9rem;
  }
  .radar {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    animation: pulse 1s infinite;
  }
  .list {
    border: 0;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .row {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 0.9rem;
    align-items: center;
    padding: 0.9rem 1.1rem;
    cursor: pointer;
  }
  .row.on {
    border-color: var(--accent);
    background: linear-gradient(120deg, var(--accent-soft), transparent 60%), var(--surface);
  }
  .row.disabled {
    cursor: not-allowed;
    opacity: 0.75;
  }
  .bulb {
    color: var(--muted);
  }
  .row.on .bulb {
    color: var(--accent);
    filter: drop-shadow(0 0 8px var(--accent));
  }
  .main {
    display: grid;
    gap: 0.4rem;
  }
  .name {
    font-weight: 700;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .others {
    margin-top: 1rem;
    color: var(--muted);
  }
  .others summary {
    cursor: pointer;
    font-weight: 700;
  }
  .others ul {
    margin: 0.5rem 0 0;
    padding-left: 1.2rem;
  }
</style>
