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
    onchange,
    onreset,
  }: {
    def: SettingDef;
    value: SettingValue;
    inherited?: boolean;
    canReset?: boolean;
    /** Topes más estrechos que el rango del catálogo (dependen de otro ajuste). */
    floor?: number;
    ceil?: number;
    onchange: (v: SettingValue) => void;
    onreset: () => void;
  } = $props();

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
</script>

<div class="setting" class:inherited data-setting={def.id}>
  <div class="head">
    {#if def.kind.kind === "number"}
      <label class="title" for={id}>{t(def.title_key)}</label>
    {:else}
      <span class="title" id="{id}-legend">{t(def.title_key)}</span>
    {/if}
    <output class="value tabular" for={id} aria-live="off">{def.kind.kind === "number" ? shown : optionLabel(String(local))}</output>
  </div>
  <p class="help" id="{id}-help">{t(def.help_key)}</p>

  {#if def.kind.kind === "number"}
    <input
      {id}
      type="range"
      min={def.kind.min}
      max={def.kind.max}
      step={def.kind.step}
      value={local}
      style:--fill={fill}
      aria-describedby="{id}-help {id}-default"
      aria-valuetext={shown}
      oninput={onInput}
      onchange={onCommit}
      onpointerup={onCommit}
      onblur={onCommit}
    />
  {:else}
    <div class="choices" role="radiogroup" aria-labelledby="{id}-legend" aria-describedby="{id}-help {id}-default">
      {#each def.kind.options as o (o)}
        <label class="choice" class:on={local === o}>
          <input type="radio" name={id} value={o} checked={local === o} onchange={() => { local = o; onchange(o); }} />
          <span>{optionLabel(o)}</span>
        </label>
      {/each}
    </div>
  {/if}

  <div class="foot">
    <span id="{id}-default" class="default">
      {t("settings.default_is", { value: def.kind.kind === "number" ? formatSetting(def.default, def.kind, i18n.locale) : optionLabel(String(def.default)) })}
    </span>
    {#if inherited}
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
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
  }
  .title {
    font-weight: 700;
  }
  .value {
    color: var(--accent);
    font-size: 0.95rem;
  }
  .help {
    color: var(--muted);
    font-size: 0.9rem;
    max-width: 62ch;
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
