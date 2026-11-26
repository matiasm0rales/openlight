<script lang="ts" module>
  // Íconos de trazo propio (24×24, stroke 1.6). Decorativos: siempre aria-hidden.
  const PATHS = {
    bulb: "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.6 10.8c.7.6 1.1 1.3 1.1 2.2h5c0-.9.4-1.6 1.1-2.2A6 6 0 0 0 12 3Z",
    home: "M4 11.5 12 5l8 6.5V20a1 1 0 0 1-1 1h-4.5v-5.5h-5V21H5a1 1 0 0 1-1-1z",
    sliders: "M5 6h9M18 6h1M5 12h3M12 12h7M5 18h11M20 18h-1M14 4v4M8 10v4M16 16v4",
    palette: "M12 3a9 9 0 1 0 0 18c1.1 0 1.6-.8 1.6-1.6 0-1.3-1-1.6-1-2.6s.8-1.4 1.8-1.4H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3ZM7.5 11.5h.01M10 7.5h.01M14.5 7.5h.01M17 11h.01",
    devices: "M7 4h10v7H7zM4 20h16M9 11v4h6v-4M12 15v5",
    pulse: "M3 12h4l2.5-6 4 12L16 9l1.5 3H21",
    target: "M12 3v3M12 18v3M3 12h3M18 12h3M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z",
    power: "M12 3v8M7.1 6.4a7 7 0 1 0 9.8 0",
    check: "m5 12.5 4.2 4.2L19 7",
    grip: "M9 5a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM15 5a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM9 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM15 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM9 17a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM15 17a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z",
    close: "M6 6l12 12M18 6 6 18",
    alert: "M12 4 2.8 19.5h18.4zM12 10v4.5M12 17.4h.01",
    chevron: "m9 5 7 7-7 7",
    back: "m15 5-7 7 7 7",
    sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2.5v2M12 19.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4",
    moon: "M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5Z",
    globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM3.5 9h17M3.5 15h17M12 3c2.5 2.6 3.5 5.6 3.5 9s-1 6.4-3.5 9c-2.5-2.6-3.5-5.6-3.5-9s1-6.4 3.5-9Z",
    key: "M14.5 9.5a4.5 4.5 0 1 0-4 4.47L12 15.5h2v2h2v2h3.5V16l-5.03-5.03c.02-.15.03-.31.03-.47ZM9.5 9.5h.01",
    wifi: "M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.5 16a5 5 0 0 1 7 0M12 19.5h.01",
    help: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM9.6 9.3a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.1-2.4 3.6M12 17h.01",
    external: "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
    download: "M12 4v11M7 10.5l5 5 5-5M5 20h14",
    play: "M8 5.5v13l10-6.5z",
    stop: "M7 7h10v10H7z",
    tray: "M4 5h16v14H4zM4 9h16M8 7h.01M11 7h.01",
    monitor: "M3 5h18v11H3zM9 20h6M12 16v4",
    plus: "M12 5v14M5 12h14",
    trash: "M5 7h14M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
    pencil: "M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17v3M14.5 7.5l3 3",
    copy: "M10 9h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1ZM15 5V4.5a.5.5 0 0 0-.5-.5h-10a.5.5 0 0 0-.5.5v10a.5.5 0 0 0 .5.5H5",
    more: "M5 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM12 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2ZM19 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z",
    refresh: "M20 11a8 8 0 0 0-14.6-4.5M4 4v3h3M4 13a8 8 0 0 0 14.6 4.5M20 20v-3h-3",
    folder: "M3.5 6.5a1 1 0 0 1 1-1h5l2 2.5h8a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1z",
    eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12ZM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z",
    phone: "M8 2.5h8a1 1 0 0 1 1 1v17a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1v-17a1 1 0 0 1 1-1ZM11 18.5h2",
    qr: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h2v2h-2zM18 18h2v2h-2zM14 18h2M18 14h2",
  } as const;
  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 20 }: { name: IconName; size?: number } = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"><path d={PATHS[name]} /></svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
