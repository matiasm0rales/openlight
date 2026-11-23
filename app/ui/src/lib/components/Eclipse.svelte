<script lang="ts">
  // El "eclipse": el aro es el color de la zona de pantalla y el disco, lo que se le mandó al foco.
  // El halo se tiñe con el color enviado, como la luz del foco en la pared.
  import type { Hsv, ScreenColor } from "../bindings";
  import { hsvCss, screenCss } from "../format";
  import { hueKey } from "../hue";
  import { t } from "../i18n/index.svelte";

  let { screen, sent, size = 120, off = false }: { screen?: ScreenColor | null; sent?: Hsv | null; size?: number; off?: boolean } = $props();

  const ring = $derived(screenCss(screen));
  const core = $derived(off || !sent ? "var(--surface-3)" : hsvCss(sent));
  const describe = (h: number | null | undefined) => (h === null || h === undefined ? t("hue.none") : t(hueKey(h * 360)));
  const label = $derived(
    t("eclipse.label", {
      screen: screen ? describe(screen.h) : t("hue.unknown"),
      sent: off || !sent ? t("eclipse.off") : describe((sent.s ?? 0) < 0.08 ? null : sent.h),
    }),
  );
</script>

<div class="eclipse" style:--size="{size}px" style:--ring={ring} style:--core={core} class:off role="img" aria-label={label}>
  <div class="halo" aria-hidden="true"></div>
  <div class="ring" aria-hidden="true"></div>
  <div class="core" aria-hidden="true"></div>
</div>

<style>
  .eclipse {
    position: relative;
    width: var(--size);
    height: var(--size);
    flex: none;
  }
  .halo,
  .ring,
  .core {
    position: absolute;
    border-radius: 50%;
    transition: background 260ms var(--ease), box-shadow 260ms var(--ease);
  }
  .halo {
    inset: -18%;
    background: radial-gradient(circle, color-mix(in srgb, var(--core) 55%, transparent) 0%, transparent 66%);
    filter: blur(10px);
    opacity: 0.85;
  }
  .off .halo {
    opacity: 0;
  }
  .ring {
    inset: 0;
    background: var(--ring);
    box-shadow: 0 0 0 1px var(--line) inset;
  }
  .core {
    inset: 17%;
    background: var(--core);
    box-shadow:
      0 0 0 3px var(--surface),
      0 0 24px -2px color-mix(in srgb, var(--core) 80%, transparent);
  }
</style>
