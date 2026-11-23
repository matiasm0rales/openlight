<script lang="ts">
  let { checked, label, onchange, busy = false, big = false, id }: { checked: boolean; label: string; onchange: (v: boolean) => void; busy?: boolean; big?: boolean; id?: string } = $props();
</script>

<button type="button" {id} role="switch" class="switch" class:big aria-checked={checked} aria-label={label} aria-busy={busy} disabled={busy} onclick={() => onchange(!checked)}>
  <span class="knob" aria-hidden="true"></span>
</button>

<style>
  .switch {
    --w: 46px;
    --h: 26px;
    position: relative;
    width: var(--w);
    height: var(--h);
    flex: none;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: var(--surface-3);
    cursor: pointer;
    transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }
  .switch.big {
    --w: 84px;
    --h: 44px;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: calc(var(--h) - 8px);
    height: calc(var(--h) - 8px);
    border-radius: 50%;
    background: var(--muted);
    transition: transform var(--dur) var(--ease), background var(--dur) var(--ease), box-shadow var(--dur);
  }
  .switch[aria-checked="true"] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .switch[aria-checked="true"] .knob {
    transform: translateX(calc(var(--w) - var(--h)));
    background: var(--accent);
    box-shadow: 0 0 18px var(--accent);
  }
  .switch[aria-busy="true"] .knob {
    animation: pulse 900ms infinite;
  }
</style>
