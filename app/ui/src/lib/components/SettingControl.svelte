<script lang="ts">
  import { untrack } from "svelte";
  // Un control generado desde un `SettingDef` del catálogo: número → slider, opción → botones.
  // No sabe nada de ajustes concretos: título, ayuda, rango, paso, unidad y default salen del catálogo.
  import type { SettingDef, SettingValue } from "../bindings";
  import { formatSetting } from "../format";
  import { i18n, t } from "../i18n/index.svelte";
  import { clampToDef, isDefault } from "../settings";
  import Icon from "./Icon.svelte";

  let {
    def,
    value,
    inherited = false,
    canReset,
    floor,
    ceil,
    note,
    overrides = [],
    source,
    onchange,
    onreset,
    onusecommon,
  }: {
    def: SettingDef;
    value: SettingValue;
    inherited?: boolean;
    canReset?: boolean;
    /** Topes más estrechos que el rango del catálogo (dependen de otro ajuste). */
    floor?: number;
    ceil?: number;
    /** Aviso: dónde no se va a notar un cambio hecho acá. */
    note?: string;
    /** En «Todos los focos»: los que tienen un valor propio y por eso no cambian con este. */
    overrides?: { device: string; name: string; value: SettingValue }[];
    /** En un foco: si el valor es propio y cuál usaría sin él (`profile` si lo pone un perfil). */
    source?: { own: boolean; common: SettingValue; profile?: string };
    onchange: (v: SettingValue) => void;
    onreset: () => void;
    /** Borra el valor propio de un foco (en «Todos los focos»). */
    onusecommon?: (device: string) => void;
  } = $props();

  let root: HTMLDivElement;
  // El botón desaparece al usarlo: el foco del teclado pasa al control del ajuste.
  function useCommon(action: () => void) {
    (root.querySelector<HTMLInputElement>("input:checked") ?? root.querySelector("input"))?.focus();
    action();
  }

  let local = $state<SettingValue>(null);
  let dragging = false;
  // El envío que espera su turno (ver `push`).
  let pending: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const next = value;
    // Solo manda lo que llega de afuera, y no mientras se arrastra o queda un cambio sin enviar:
    // la respuesta a uno anterior haría volver atrás al control.
    untrack(() => {
      if (!dragging && pending === null) local = next;
    });
  });

  const id = $derived(`setting-${def.id}`);
  const shown = $derived(formatSetting(local, def.kind, i18n.locale));
  const atDefault = $derived(isDefault(def, value));
  const fill = $derived.by(() => {
    if (def.kind.kind !== "number" || typeof local !== "number") return "0%";
    const min = def.kind.min ?? 0;
    const max = def.kind.max ?? 1;
    return `${((local - min) / (max - min || 1)) * 100}%`;
  });

  // Vista previa en vivo: se manda mientras se arrastra, como mucho cada 120 ms.
  let last = 0;
  function push(v: SettingValue) {
    const now = performance.now();
    if (pending) clearTimeout(pending);
    pending = null;
    if (now - last > 120) {
      last = now;
      onchange(v);
    } else {
      pending = setTimeout(() => {
        pending = null;
        last = performance.now();
        onchange(v);
      }, 120);
    }
  }

  function onInput(e: Event) {
    dragging = true;
    const input = e.currentTarget as HTMLInputElement;
    const v = Math.min(ceil ?? Infinity, Math.max(floor ?? -Infinity, clampToDef(def, Number(input.value))));
    // La perilla se frena en el tope, aunque el recorrido siga.
    input.value = String(v);
    local = v;
    push(v);
  }
  function onCommit() {
    dragging = false;
  }
  const optionLabel = (o: string) => t(`settings.${def.id}.option.${o}`);
  const show = (v: SettingValue) => (def.kind.kind === "number" ? formatSetting(v, def.kind, i18n.locale) : optionLabel(String(v)));
  const describedBy = $derived(
    [`${id}-help`, ...overrides.map((_, i) => `${id}-override-${i}`), source?.own ? `${id}-own` : "", `${id}-default`].filter(Boolean).join(" "),
  );
</script>

<div class="setting" class:inherited class:overridden={overrides.length > 0} data-setting={def.id} bind:this={root}>
  <div class="head">
    <div class="name">
      {#if def.kind.kind === "number"}
        <label class="title" for={id}>{t(def.title_key)}</label>
      {:else}
        <span class="title" id="{id}-legend">{t(def.title_key)}</span>
      {/if}
      {#if source}
        <span class="chip source" class:accent={source.own}>
          {source.own ? t("settings.own") : source.profile ? t("settings.from_profile", { name: source.profile }) : t("settings.from_common")}
        </span>
      {/if}
    </div>
    <output class="value tabular" for={id} aria-live="off">{def.kind.kind === "number" ? shown : optionLabel(String(local))}</output>
  </div>
  <p class="help" id="{id}-help">{t(def.help_key)}</p>
  {#if note}<p class="note">{note}</p>{/if}
  {#if overrides.length}
    <ul class="overrides">
      {#each overrides as o, i (o.device)}
        <li>
          <span class="mark" aria-hidden="true"><Icon name="alert" size={16} /></span>
          <span id="{id}-override-{i}">{t("settings.device_override", { name: o.name, value: show(o.value) })}</span>
          <button type="button" class="btn small" onclick={() => useCommon(() => onusecommon?.(o.device))} aria-label={t("settings.use_common_on", { device: o.name, name: t(def.title_key) })}>
            {t("settings.use_common")}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if source?.own}
    <p class="note" id="{id}-own">
      {source.profile ? t("settings.without_own_profile", { value: show(source.common), name: source.profile }) : t("settings.without_own", { value: show(source.common) })}
    </p>
  {/if}

  {#if def.kind.kind === "number"}
    <input
      {id}
      type="range"
      min={def.kind.min}
      max={def.kind.max}
      step={def.kind.step}
      value={local}
      style:--fill={fill}
      aria-describedby={describedBy}
      aria-valuetext={shown}
      oninput={onInput}
      onchange={onCommit}
      onpointerup={onCommit}
      onblur={onCommit}
    />
  {:else}
    <div class="choices" role="radiogroup" aria-labelledby="{id}-legend" aria-describedby={describedBy}>
      {#each def.kind.options as o (o)}
        <label class="choice" class:on={local === o}>
          <input type="radio" name={id} value={o} checked={local === o} onchange={() => { local = o; onchange(o); }} />
          <span>{optionLabel(o)}</span>
        </label>
      {/each}
    </div>
  {/if}

  <div class="foot">
    <span id="{id}-default" class="default">{t("settings.default_is", { value: show(def.default) })}</span>
    {#if source}
      {#if source.own}
        <button type="button" class="btn small" onclick={() => useCommon(onreset)} aria-label={t("settings.use_common_named", { name: t(def.title_key) })}>
          <Icon name="refresh" size={15} />{t("settings.use_common")}
        </button>
      {/if}
    {:else if inherited}
      <span class="chip">{t("settings.inherited")}</span>
    {:else}
      <button type="button" class="btn ghost small" disabled={!(canReset ?? !atDefault)} onclick={onreset} aria-label={t("settings.restore_named", { name: t(def.title_key) })}>
        <Icon name="refresh" size={15} />{t("settings.restore_default")}
      </button>
    {/if}
  </div>
</div>

<style>
  .setting {
    display: grid;
    /* En una fila con otra tarjeta más alta, el contenido no se estira. */
    align-content: start;
    gap: 0.45rem;
    padding: 1rem 1.1rem;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .setting.inherited {
    background: transparent;
    border-style: dashed;
  }
  .setting.overridden {
    border-color: color-mix(in srgb, var(--warn) 55%, var(--line));
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
  }
  .name {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }
  .source {
    white-space: normal;
  }
  .overrides {
    display: grid;
    gap: 0.4rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .overrides li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.4rem 0.55rem;
    padding: 0.6rem 0.7rem;
    border-radius: var(--radius-sm);
    background: var(--warn-soft);
    font-size: 0.86rem;
    font-weight: 600;
  }
  .overrides .mark {
    color: var(--warn);
    padding-top: 0.1rem;
  }
  .overrides button {
    grid-column: 2;
    justify-self: start;
  }
  .title {
    font-weight: 700;
  }
  .value {
    white-space: nowrap;
    color: var(--accent);
    font-size: 0.95rem;
  }
  .help {
    color: var(--muted);
    font-size: 0.9rem;
    max-width: 62ch;
  }
  .note {
    justify-self: start;
    padding: 0.35rem 0.6rem;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--text);
    font-size: 0.82rem;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.2rem;
  }
  .choice {
    position: relative;
    display: inline-flex;
    padding: 0.45em 0.95em;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    cursor: pointer;
    font-weight: 700;
    font-size: 0.9rem;
  }
  .choice input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: pointer;
  }
  .choice.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
  .choice:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 3px;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    min-height: 32px;
  }
  .default {
    font-size: 0.82rem;
    color: var(--faint);
  }
</style>
