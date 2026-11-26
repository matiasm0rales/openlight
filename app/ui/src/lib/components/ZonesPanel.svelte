<script lang="ts">
  // Monitores con vista previa en vivo, una distribución para repartir el monitor entre todos los
  // focos y la zona de cada uno (paso 9 y pantalla Dispositivos).
  import type { DisplayInfo, UserFacingError, Zone } from "../bindings";
  import { attempt, commands } from "../ipc";
  import { app } from "../app.svelte";
  import { screenCss } from "../format";
  import { everyWhileVisible } from "../visibility";
  import { t } from "../i18n/index.svelte";
  import { LAYOUTS, layoutZones, layoutsFor, matchesLayout, rectOf, type LayoutId } from "../zones";
  import ErrorCard from "./ErrorCard.svelte";
  import Icon from "./Icon.svelte";
  import ZoneEditor from "./ZoneEditor.svelte";

  let displays = $state<DisplayInfo[]>([]);
  let chosen = $state<string | null>(null);
  let previews = $state<Record<string, string>>({});
  let error = $state<UserFacingError | null>(null);
  let sampled = $state<Record<string, string>>({});

  async function loadDisplays() {
    const r = await attempt(() => commands.displaysList());
    if (!r.ok) return (error = r.error);
    displays = r.value;
    // Por defecto: el monitor que ya usan los focos, o el primero externo (como elegir_monitor).
    chosen ??= app.devices[0]?.zones[0]?.display ?? (r.value.find((d) => !d.builtin) ?? r.value[0])?.id ?? null;
    for (const d of r.value) void loadPreview(d.id);
  }

  async function loadPreview(id: string) {
    const r = await attempt(() => commands.displayPreview(id));
    if (r.ok) previews[id] = `data:image/png;base64,${r.value.png_base64}`;
    else error = r.error;
  }

  $effect(() => {
    void loadDisplays();
    // La vista previa es una captura entera: con la ventana oculta no se pide
    return everyWhileVisible(() => chosen && loadPreview(chosen), 2500);
  });

  const display = $derived(displays.find((d) => d.id === chosen));

  // Color aproximado de cada zona sacado de la vista previa, mientras el efecto no está andando.
  // Cuando hay sesión, se usa el color real que manda el backend (`color://live`).
  $effect(() => {
    const src = chosen ? previews[chosen] : null;
    if (!src || typeof document === "undefined") return;
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      c.width = 48;
      c.height = 27;
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.drawImage(img, 0, 0, 48, 27);
      for (const d of app.devices) {
        const r = rectOf(d.zones.find((z) => z.display === chosen));
        const w = Math.max(1, Math.round(r.w * 48));
        const h = Math.max(1, Math.round(r.h * 27));
        const px = ctx.getImageData(Math.round(r.x * 48), Math.round(r.y * 27), w, h).data;
        let [R, G, B, n] = [0, 0, 0, 0];
        for (let i = 0; i < px.length; i += 4) {
          R += px[i];
          G += px[i + 1];
          B += px[i + 2];
          n++;
        }
        sampled[d.id] = `rgb(${Math.round(R / n)} ${Math.round(G / n)} ${Math.round(B / n)})`;
      }
    };
    img.src = src;
  });

  const items = $derived(
    app.devices.map((d) => ({
      id: d.id,
      name: d.name,
      zone: d.zones.find((z) => z.display === chosen),
      color: app.live[d.id] ? screenCss(app.live[d.id].screen) : (sampled[d.id] ?? "var(--accent)"),
    })),
  );

  const pending = new Map<string, ReturnType<typeof setTimeout>>();
  function change(device: string, zone: Zone) {
    const d = app.devices.find((x) => x.id === device);
    if (!d) return;
    // Solo se reemplaza la zona de este monitor: las de los otros monitores se conservan.
    d.zones = [...d.zones.filter((z) => z.display !== zone.display), zone];
    clearTimeout(pending.get(device));
    pending.set(
      device,
      setTimeout(async () => {
        const zones = $state.snapshot(d.zones);
        const r = await attempt(() => commands.zonesSet(device, zones));
        if (!r.ok) error = r.error;
      }, 200),
    );
  }

  const layouts = $derived(layoutsFor(app.devices.length));
  const inOrder = $derived(app.devices.map((d, i) => `${i + 1} ${d.name}`).join(", "));
  // Reparte el monitor elegido entre todos los focos, en el orden del panel. Cada uno se guarda
  // como cualquier cambio del editor y después se puede ajustar a mano.
  function distribute(layout: LayoutId, on: string) {
    for (const { device, zone } of layoutZones(layout, app.devices.map((d) => d.id), on)) change(device, zone);
  }
  const share = (n: number) => `${n * 100}%`;
</script>

{#if error}<ErrorCard {error} retry={() => { error = null; return loadDisplays(); }} />{/if}

<div class="zones">
  <div class="displays" role="radiogroup" aria-label={t("zones.choose_display")}>
    {#each displays as d (d.id)}
      <label class="display" class:on={chosen === d.id}>
        <input type="radio" name="display" value={d.id} bind:group={chosen} />
        <span class="thumb" style:aspect-ratio="{d.width} / {d.height}">
          {#if previews[d.id]}<img src={previews[d.id]} alt="" />{/if}
        </span>
        <span class="dname"><Icon name="monitor" size={16} />{d.name}</span>
        <span class="muted small mono">{d.width}×{d.height}{d.builtin ? ` · ${t("zones.builtin")}` : ""}{d.primary ? ` · ${t("zones.primary")}` : ""}</span>
      </label>
    {/each}
  </div>

  {#if display && layouts.length}
    <div class="layouts" role="group" aria-labelledby="layout-title" aria-describedby="layout-order">
      <p id="layout-title" class="label">{t("zones.layout_title")}</p>
      <div class="options">
        {#each layouts as id (id)}
          <button type="button" class="layout" aria-pressed={matchesLayout(id, app.devices, display.id)} onclick={() => distribute(id, display.id)}>
            <span class="screen" aria-hidden="true">
              {#each LAYOUTS[id] as { rect }, i (i)}
                <span class="cell" style:left={share(rect.x)} style:top={share(rect.y)} style:width={share(rect.w)} style:height={share(rect.h)}><span>{i + 1}</span></span>
              {/each}
            </span>
            {t(`zones.layout.${id}`)}
          </button>
        {/each}
      </div>
      <p id="layout-order" class="muted order">{t("zones.layout_order", { list: inOrder })}</p>
    </div>
  {/if}

  {#if display}
    <ZoneEditor {display} preview={previews[display.id] ?? null} {items} onchange={change} />
  {/if}
</div>

<style>
  .zones {
    display: grid;
    gap: 1.2rem;
  }
  .displays {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
  }
  .display {
    position: relative;
    display: grid;
    gap: 0.3rem;
    width: 190px;
    padding: 0.6rem;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: var(--surface);
    cursor: pointer;
  }
  .display input {
    position: absolute;
    opacity: 0;
  }
  .display:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 3px;
  }
  .display.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .thumb {
    display: block;
    border-radius: 6px;
    overflow: hidden;
    background: #000;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .dname {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-weight: 700;
    font-size: 0.9rem;
  }
  .small {
    font-size: 0.75rem;
  }
  .layouts {
    display: grid;
    gap: 0.6rem;
  }
  .label {
    font-weight: 700;
    font-size: 0.9rem;
  }
  .options {
    display: flex;
    flex-wrap: wrap;
    gap: 0.7rem;
  }
  /* Un monitor en miniatura, numerado: se ve de antemano qué lugar le toca a cada foco. */
  .layout {
    display: grid;
    gap: 0.45rem;
    justify-items: center;
    align-content: start;
    width: 150px;
    padding: 0.6rem 0.6rem 0.55rem;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font: 600 0.82rem/1.3 var(--font-body);
    cursor: pointer;
  }
  .layout[aria-pressed="true"] {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .screen {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
  }
  .cell {
    position: absolute;
    padding: 1.5px;
  }
  .cell span {
    display: grid;
    place-items: center;
    height: 100%;
    border-radius: 5px;
    background: var(--surface-3);
    font: 500 0.8rem var(--font-mono);
  }
  .layout[aria-pressed="true"] .cell span {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .order {
    font-size: 0.82rem;
  }
</style>
