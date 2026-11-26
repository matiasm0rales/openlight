<script lang="ts">
  import Icon from "./Icon.svelte";

  let { checked, label, onchange, busy = false, big = false, disabled = false, id }: { checked: boolean; label: string; onchange: (v: boolean) => void; busy?: boolean; big?: boolean; disabled?: boolean; id?: string } = $props();
</script>

<button type="button" {id} role="switch" class="switch" class:big aria-checked={checked} aria-label={label} aria-busy={busy} disabled={busy || disabled} onclick={() => onchange(!checked)}>
  <span class="knob" aria-hidden="true"><Icon name="power" size={big ? 18 : 12} /></span>
</button>

<style>
  .switch {
    --w: 52px;
    --h: 30px;
    position: relative;
    width: var(--w);
    height: var(--h);
    flex: none;
    padding: 0;
    border-radius: 999px;
    border: 0;
    background: var(--surface-3);
    cursor: pointer;
    transition: background var(--dur) var(--ease);
  }
  .switch.big {
    --w: 88px;
    --h: 48px;
  }
  .knob {
    position: absolute;
    top: 4px;
    left: 4px;
    display: grid;
    place-items: center;
    width: calc(var(--h) - 8px);
    height: calc(var(--h) - 8px);
    border-radius: 50%;
    background: #fff;
    color: var(--faint);
    box-shadow: 0 2px 6px rgb(20 22 28 / 0.25);
    transition: transform var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  /* Encendido: pista del color del foco (o tinta en reposo) y perilla blanca con el ícono de encendido. */
  .switch[aria-checked="true"] {
    background: var(--live);
    box-shadow: 0 8px 22px -8px var(--live);
    /* El color en vivo cambia 10 veces por segundo: sin transición, que no dejaría de redibujar */
    transition: none;
  }
  .switch[aria-checked="true"] .knob {
    transform: translateX(calc(var(--w) - var(--h)));
    color: #16181d;
  }
  .switch:disabled:not([aria-busy="true"]) {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .switch[aria-busy="true"] .knob {
    animation: pulse 900ms infinite;
  }
</style>
