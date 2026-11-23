<script lang="ts">
  import { app } from "../../lib/app.svelte";
  import ZonesPanel from "../../lib/components/ZonesPanel.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();
  $effect(() => {
    valid = app.devices.length > 0 && app.devices.every((d) => d.zones.length > 0);
  });
</script>

<ZonesPanel />
