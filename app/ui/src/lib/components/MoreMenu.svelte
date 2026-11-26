<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";

  export interface MenuItem {
    label: string;
    icon: IconName;
    run: () => unknown;
  }
</script>

<script lang="ts">
  // Un botón que despliega acciones secundarias (patrón «menu button» de WAI-ARIA): las flechas,
  // Inicio y Fin recorren los ítems, Esc cierra y vuelve al botón, y Tab o un clic afuera cierran.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  /** `text` es lo que se ve; `label`, el nombre completo para lectores de pantalla. */
  let { text, label, items }: { text: string; label: string; items: MenuItem[] } = $props();

  const id = $props.id();
  let open = $state(false);
  let root: HTMLElement | undefined = $state();
  let button: HTMLButtonElement | undefined = $state();

  const entries = () => [...(root?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])];

  async function show(at = 0) {
    open = true;
    await tick();
    entries().at(at)?.focus();
  }

  function close() {
    open = false;
    button?.focus();
  }

  function choose(item: MenuItem) {
    close();
    void item.run();
  }

  function onButtonKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    void show(e.key === "ArrowDown" ? 0 : -1);
  }

  function onMenuKey(e: KeyboardEvent) {
    const list = entries();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const to = ({ ArrowDown: (i + 1) % list.length, ArrowUp: i - 1, Home: 0, End: -1 } as Record<string, number>)[e.key];
    if (to !== undefined) {
      e.preventDefault();
      list.at(to)?.focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
      // Con Tab no se frena el evento: sigue desde el botón.
    } else if (e.key === "Tab") close();
  }

  const outside = (target: EventTarget | null) => target instanceof Node && !root?.contains(target);
</script>

<!-- `pointerdown` y no solo el foco: en WebKit un clic no le da foco a un botón. -->
<svelte:window onpointerdown={(e) => open && outside(e.target) && (open = false)} />

<div class="more" bind:this={root} onfocusout={(e) => open && outside(e.relatedTarget) && (open = false)}>
  <button
    bind:this={button}
    type="button"
    class="btn secondary small"
    id="{id}-button"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={open ? `${id}-menu` : undefined}
    aria-label={label}
    onclick={() => (open ? close() : show())}
    onkeydown={onButtonKey}
  >
    <Icon name="more" size={15} />{text}
  </button>
  {#if open}
    <div class="menu" id="{id}-menu" role="menu" aria-labelledby="{id}-button" tabindex="-1" onkeydown={onMenuKey}>
      {#each items as item (item.label)}
        <button type="button" role="menuitem" tabindex="-1" onclick={() => choose(item)}><Icon name={item.icon} size={15} />{item.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .more {
    position: relative;
  }
  .more > .btn {
    padding-inline: 0.8em;
  }
  .menu {
    position: absolute;
    z-index: 5;
    top: calc(100% + 6px);
    right: 0;
    display: grid;
    gap: 2px;
    min-width: 11rem;
    padding: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: 0 16px 32px -16px rgb(0 0 0 / 0.4);
  }
  .menu button {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    min-height: 40px;
    padding: 0.4rem 0.7rem;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--text);
    font: 600 0.88rem var(--font-body);
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--surface-3);
  }
  .menu button:focus-visible {
    outline-offset: -3px;
  }
</style>
