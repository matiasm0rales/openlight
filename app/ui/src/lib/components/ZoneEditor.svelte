<script lang="ts">
  // Vista previa del monitor con la zona de cada foco. Atajos (toda, izquierda...) o rectángulo
  // a mano: se arrastra con el mouse o se mueve con el teclado (flechas mueven, Mayús+flechas
  // cambian el tamaño). Cada zona muestra en vivo el color que calcula.
  import type { DisplayInfo, Zone } from "../bindings";
  import { t } from "../i18n/index.svelte";
  import { PRESETS, constrain, presetZone, rectOf, type Rect } from "../zones";

  interface Item {
    id: string;
    name: string;
    zone: Zone | undefined;
    color: string;
  }

  let {
    display,
    preview,
    items,
    onchange,
  }: {
    display: DisplayInfo;
    preview: string | null;
    items: Item[];
    onchange: (device: string, zone: Zone) => void;
  } = $props();

  let selected = $state<string | null>(null);
  const current = $derived(items.find((i) => i.id === (selected ?? items[0]?.id)));
  let frame: HTMLDivElement | undefined = $state();
  let drag: { id: string; mode: "move" | "resize"; start: Rect; px: number; py: number } | null = null;

  function setRect(id: string, r: Rect) {
    onchange(id, { display: display.id, rect: constrain(r), preset: "custom" });
  }

  function pointerDown(e: PointerEvent, item: Item, mode: "move" | "resize") {
    if (!frame) return;
    selected = item.id;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { id: item.id, mode, start: rectOf(item.zone), px: e.clientX, py: e.clientY };
    e.stopPropagation();
  }

  function pointerMove(e: PointerEvent) {
    if (!drag || !frame) return;
    const box = frame.getBoundingClientRect();
    const dx = (e.clientX - drag.px) / box.width;
    const dy = (e.clientY - drag.py) / box.height;
    const s = drag.start;
    setRect(drag.id, drag.mode === "move" ? { ...s, x: s.x + dx, y: s.y + dy } : { ...s, w: s.w + dx, h: s.h + dy });
  }

  function pointerUp() {
    drag = null;
  }

  function key(e: KeyboardEvent, item: Item) {
    const step = e.altKey ? 0.01 : 0.05;
    const r = rectOf(item.zone);
    const d: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    const v = d[e.key];
    if (!v) return;
    e.preventDefault();
    setRect(item.id, e.shiftKey ? { ...r, w: r.w + v[0], h: r.h + v[1] } : { ...r, x: r.x + v[0], y: r.y + v[1] });
  }

  const pct = (n: number) => `${Math.round(n * 100)}%`;
  const ratio = $derived(`${display.width} / ${display.height}`);
</script>

<div class="editor">
  <div class="tabs" role="group" aria-label={t("zones.devices")}>
    {#each items as item (item.id)}
      <button
        type="button"
        class="tab"
        aria-pressed={current?.id === item.id}
        onclick={() => (selected = item.id)}
      >
        <i style:background={item.color} aria-hidden="true"></i>{item.name}
      </button>
    {/each}
  </div>

  <div class="frame" bind:this={frame} style:aspect-ratio={ratio} onpointermove={pointerMove} onpointerup={pointerUp} role="presentation">
    {#if preview}
      <img src={preview} alt={t("zones.preview_alt", { name: display.name })} />
    {:else}
      <div class="placeholder"><span class="spinner" aria-hidden="true"></span>{t("zones.loading_preview")}</div>
    {/if}
    {#each items as item (item.id)}
      {@const r = rectOf(item.zone)}
      <div
        class="zone"
        class:active={current?.id === item.id}
        style:left={pct(r.x)}
        style:top={pct(r.y)}
        style:width={pct(r.w)}
        style:height={pct(r.h)}
        style:--c={item.color}
        style:z-index={current?.id === item.id ? 3 : 2}
        role="slider"
        tabindex="0"
        aria-label={t("zones.rect_label", { name: item.name })}
        aria-valuetext={t("zones.rect_value", { x: pct(r.x), y: pct(r.y), w: pct(r.w), h: pct(r.h) })}
        aria-valuenow={Math.round(r.x * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-describedby="zone-keys"
        onfocus={() => (selected = item.id)}
        onkeydown={(e) => key(e, item)}
        onpointerdown={(e) => pointerDown(e, item, "move")}
      >
        <span class="tag">{item.name}</span>
        <span class="swatch" aria-hidden="true"></span>
        <span class="handle" aria-hidden="true" onpointerdown={(e) => current && pointerDown(e, item, "resize")}></span>
      </div>
    {/each}
  </div>
  <p id="zone-keys" class="keys muted">{t("zones.keys")}</p>

  {#if current}
    <div id="zone-presets" class="presets" role="group" aria-label={t("zones.presets_for", { name: current.name })}>
      {#each PRESETS as p (p)}
        <button type="button" class="preset" aria-pressed={current.zone?.preset === p} onclick={() => onchange(current.id, presetZone(p, display.id))}>
          <span class="mini" aria-hidden="true"><i style:left={pct(presetZone(p, "").rect.x ?? 0)} style:top={pct(presetZone(p, "").rect.y ?? 0)} style:width={pct(presetZone(p, "").rect.w ?? 1)} style:height={pct(presetZone(p, "").rect.h ?? 1)}></i></span>
          {t(`zones.preset.${p}`)}
        </button>
      {/each}
      {#if current.zone?.preset === "custom"}<span class="preset custom on">{t("zones.preset.custom")}</span>{/if}
    </div>
  {/if}
</div>

<style>
  .editor {
    display: grid;
    gap: 0.8rem;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4em 0.9em;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: 700 0.9rem var(--font-body);
    cursor: pointer;
  }
  .tab i {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .tab[aria-pressed="true"] {
    background: var(--surface-3);
    border-color: var(--accent);
  }
  .frame {
    position: relative;
    width: 100%;
    max-height: 46vh;
    border-radius: 10px;
    overflow: hidden;
    background: #000;
    border: 6px solid #0b0a08;
    box-shadow: 0 0 0 1px var(--line-strong), 0 26px 50px -30px rgb(0 0 0 / 0.9);
    touch-action: none;
    user-select: none;
  }
  .frame img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.85;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    gap: 0.5rem;
    justify-items: center;
    color: #cfc6b8;
  }
  .zone {
    position: absolute;
    border: 2px dashed rgb(255 255 255 / 0.55);
    background: color-mix(in srgb, var(--c) 14%, transparent);
    cursor: grab;
  }
  .zone.active {
    border: 2px solid var(--c);
    background: color-mix(in srgb, var(--c) 22%, transparent);
    box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.28);
  }
  .zone:focus-visible {
    outline: 3px solid var(--focus);
    outline-offset: -5px;
  }
  .tag {
    position: absolute;
    left: 6px;
    top: 6px;
    padding: 0.1em 0.5em;
    border-radius: 6px;
    background: rgb(11 10 8 / 0.8);
    color: #f3ede2;
    font-size: 0.75rem;
    font-weight: 700;
  }
  .swatch {
    position: absolute;
    right: 8px;
    top: 8px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 14px var(--c), 0 0 0 2px rgb(11 10 8 / 0.8);
  }
  .handle {
    position: absolute;
    right: -2px;
    bottom: -2px;
    width: 18px;
    height: 18px;
    background: var(--c);
    border-radius: 4px 0 6px 0;
    cursor: nwse-resize;
  }
  .keys {
    font-size: 0.82rem;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .preset {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35em 0.8em 0.35em 0.45em;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    font: 0.88rem var(--font-body);
    cursor: pointer;
  }
  .preset[aria-pressed="true"],
  .preset.custom.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .preset.custom {
    cursor: default;
    opacity: 0.6;
  }
  .preset.custom.on {
    opacity: 1;
  }
  .mini {
    position: relative;
    width: 26px;
    height: 16px;
    border-radius: 3px;
    border: 1px solid var(--line-strong);
  }
  .mini i {
    position: absolute;
    background: var(--accent);
    opacity: 0.7;
  }
</style>
