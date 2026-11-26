<script lang="ts">
  // El asa de un foco en el panel. Tres formas de cambiarlo de lugar: arrastrarla, las flechas del
  // teclado, o activarla para ver «Mover antes» y «Mover después» (para quien no puede arrastrar y
  // para los lectores de pantalla, que se quedan con las flechas). Acá no se sabe nada de la lista:
  // se avisa qué se pidió y la pantalla decide.
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let {
    id,
    name,
    first,
    last,
    describedby,
    onmove,
    ondrag,
    ondrop,
    oncancel,
  }: {
    id: string;
    name: string;
    first: boolean;
    last: boolean;
    describedby: string;
    onmove: (step: -1 | 1) => void;
    /** Se está arrastrando y el puntero está en (`x`, `y`) de la ventana. */
    ondrag: (x: number, y: number) => void;
    ondrop: () => void;
    oncancel: () => void;
  } = $props();

  let root: HTMLDivElement | undefined = $state();
  let handle: HTMLButtonElement | undefined = $state();
  let open = $state(false);

  /** Cuánto tiene que moverse el puntero para que sea un arrastre y no un toque. */
  const SLOP = 6;
  /** Franja, arriba y abajo de la ventana, donde arrastrar desplaza la página. */
  const EDGE = 80;
  const SPEED = 18;

  let press: { pointer: number; x: number; y: number } | null = null;
  let dragging = $state(false);
  /** El último toque terminó siendo un arrastre: su `click` no cuenta. */
  let dragged = false;
  let at = { x: 0, y: 0 };
  let frame = 0;

  function grab(e: PointerEvent) {
    if (e.button !== 0 || !e.isPrimary) return;
    dragged = false;
    press = { pointer: e.pointerId, x: e.clientX, y: e.clientY };
    handle?.setPointerCapture(e.pointerId);
  }

  function drag(e: PointerEvent) {
    if (!press || e.pointerId !== press.pointer) return;
    at = { x: e.clientX, y: e.clientY };
    if (!dragging) {
      if (Math.hypot(at.x - press.x, at.y - press.y) < SLOP) return;
      dragging = dragged = true;
      open = false;
      frame = requestAnimationFrame(scroll);
    }
    ondrag(at.x, at.y);
  }

  // Cerca de un borde la página se desplaza sola, para llegar a los focos que no se ven.
  function scroll() {
    const near = Math.min(0, at.y - EDGE) + Math.max(0, at.y - (window.innerHeight - EDGE));
    if (near) {
      window.scrollBy(0, Math.round(Math.max(-1, Math.min(1, near / EDGE)) * SPEED));
      ondrag(at.x, at.y);
    }
    frame = requestAnimationFrame(scroll);
  }

  function end(finish: () => void) {
    const was = dragging;
    press = null;
    dragging = false;
    cancelAnimationFrame(frame);
    if (was) finish();
  }

  // Si la tarjeta desaparece a mitad de un arrastre no queda nada andando.
  $effect(() => () => cancelAnimationFrame(frame));

  function close() {
    open = false;
    handle?.focus();
  }

  function key(e: KeyboardEvent) {
    const step = e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : null;
    if (step === null || e.altKey || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    onmove(step);
  }

  function escape(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (press) {
      e.preventDefault();
      dragged = true;
      end(oncancel);
    } else if (open) {
      e.preventDefault();
      close();
    }
  }
</script>

<svelte:window onkeydown={escape} onpointerdown={(e) => open && !root?.contains(e.target as Node) && (open = false)} />

<!-- Si el foco del teclado se va a otra parte, los botones se guardan. Al mover la tarjeta se pierde
     un instante sin ir a ningún lado: eso no los cierra. -->
<div class="reorder" bind:this={root} onfocusout={(e) => e.relatedTarget && !root?.contains(e.relatedTarget as Node) && (open = false)}>
  <button
    bind:this={handle}
    type="button"
    class="grip"
    class:dragging
    aria-label={t("dashboard.reorder", { name })}
    aria-describedby={describedby}
    aria-expanded={open}
    aria-controls="move-{id}"
    aria-keyshortcuts="ArrowUp ArrowDown"
    title={t("dashboard.reorder_tooltip")}
    onclick={(e) => (e.detail === 0 || !dragged) && (open = !open)}
    onkeydown={key}
    onpointerdown={grab}
    onpointermove={drag}
    onpointerup={() => end(ondrop)}
    onpointercancel={() => end(oncancel)}
    onlostpointercapture={() => press && end(oncancel)}
  >
    <Icon name="grip" />
  </button>
  <div id="move-{id}" class="moves" role="group" aria-label={t("dashboard.reorder", { name })} hidden={!open}>
    <button type="button" class="btn small" aria-disabled={first} onclick={() => onmove(-1)}><Icon name="back" size={15} />{t("dashboard.move_before")}</button>
    <button type="button" class="btn small" aria-disabled={last} onclick={() => onmove(1)}>{t("dashboard.move_after")}<Icon name="chevron" size={15} /></button>
  </div>
</div>

<style>
  .reorder {
    position: relative;
  }
  .grip {
    display: grid;
    place-items: center;
    width: 40px;
    height: 44px;
    margin: -7px 0 -7px -10px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    cursor: grab;
    touch-action: none;
    -webkit-user-select: none;
    user-select: none;
  }
  .grip:hover,
  .grip[aria-expanded="true"],
  .grip.dragging {
    background: var(--surface-2);
    color: var(--text);
  }
  .grip.dragging {
    cursor: grabbing;
  }
  .moves {
    position: absolute;
    z-index: 5;
    top: calc(100% + 12px);
    left: -10px;
    display: flex;
    gap: 0.4rem;
    padding: 0.4rem;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    box-shadow: var(--shadow);
    white-space: nowrap;
  }
  .moves[hidden] {
    display: none;
  }
</style>
