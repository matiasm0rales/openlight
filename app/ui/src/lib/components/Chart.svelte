<script lang="ts">
  // Gráfico de líneas en SVG, sin dependencias. Para lectores de pantalla hay una tabla resumida.
  import { i18n, t } from "../i18n/index.svelte";

  interface Series {
    label: string;
    color: string;
    values: number[];
  }
  let { title, series, unit, max, points = 60 }: { title: string; series: Series[]; unit: string; max?: number; points?: number } = $props();

  const W = 300;
  const H = 110;
  const top = $derived(Math.max(1, max ?? 0, ...series.flatMap((s) => s.values)) * 1.1);
  const nf = $derived(new Intl.NumberFormat(i18n.locale, { maximumFractionDigits: 1 }));

  function path(values: number[]): string {
    const n = values.length;
    if (n === 0) return "";
    const dx = W / (points - 1);
    const offset = points - n;
    return values.map((v, i) => `${i ? "L" : "M"}${((offset + i) * dx).toFixed(1)},${(H - (v / top) * H).toFixed(1)}`).join(" ");
  }
  const avg = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0);
</script>

<figure class="chart">
  <figcaption>
    <span class="eyebrow">{title}</span>
    <span class="legend">
      {#each series as s (s.label)}
        <span class="key"><i style:background={s.color}></i>{s.label} <b class="tabular">{nf.format(s.values.at(-1) ?? 0)}</b> <small>{unit}</small></span>
      {/each}
    </span>
  </figcaption>
  <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
    {#each [0.25, 0.5, 0.75] as g (g)}
      <line x1="0" x2={W} y1={H * g} y2={H * g} class="grid" />
    {/each}
    {#each series as s (s.label)}
      <path d={path(s.values)} stroke={s.color} class="line" />
    {/each}
  </svg>
  <table class="sr-only">
    <caption>{title}</caption>
    <thead><tr><th scope="col">{t("chart.series")}</th><th scope="col">{t("chart.now")}</th><th scope="col">{t("chart.average")}</th></tr></thead>
    <tbody>
      {#each series as s (s.label)}
        <tr><th scope="row">{s.label}</th><td>{nf.format(s.values.at(-1) ?? 0)} {unit}</td><td>{nf.format(avg(s.values))} {unit}</td></tr>
      {/each}
    </tbody>
  </table>
</figure>

<style>
  .chart {
    margin: 0;
    display: grid;
    gap: 0.6rem;
  }
  figcaption {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 0.5rem;
    align-items: baseline;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
  .key i {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .key b {
    color: var(--text);
  }
  svg {
    width: 100%;
    height: 130px;
    border-radius: var(--radius-sm);
    background: var(--bg-deep);
  }
  .grid {
    stroke: var(--line);
    stroke-dasharray: 2 4;
    vector-effect: non-scaling-stroke;
  }
  .line {
    fill: none;
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
    stroke-linejoin: round;
  }
</style>
