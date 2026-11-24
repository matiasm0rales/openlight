<script lang="ts">
  // Selector de idioma para pantallas sin riel (el asistente inicial). Cada idioma se muestra con
  // su propio nombre, sin traducir, para que se reconozca aunque la pantalla esté en otro idioma.
  import { i18n, locales, setLocale, type Locale } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  const NAMES: Record<Locale, string> = { es: "Español", en: "English" };
</script>

<!-- La etiqueta va en los dos idiomas a propósito: quien no entiende el idioma actual igual la entiende. -->
<div class="lang" role="group" aria-label="Idioma / Language" data-testid="language-switch">
  <Icon name="globe" size={16} />
  <div class="seg">
    {#each locales() as l (l)}
      <button type="button" lang={l} aria-pressed={i18n.locale === l} onclick={() => setLocale(l)}>{NAMES[l]}</button>
    {/each}
  </div>
</div>

<style>
  .lang {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    color: var(--muted);
  }
  .seg {
    flex: 1;
    display: flex;
    padding: 4px;
    border-radius: 999px;
    /* La barra de pasos es --bg-deep: sobre --surface-2 la píldora no se distinguiría. */
    background: var(--surface);
  }
  .seg button {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 32px;
    padding: 0 0.7rem;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: 700 0.82rem var(--font-body);
    cursor: pointer;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .seg button[aria-pressed="true"] {
    background: var(--accent);
    color: var(--accent-ink);
  }
</style>
