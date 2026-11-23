<script lang="ts">
  // Guías de ayuda (contenido editable en src/content/guide). Se abren desde las acciones
  // `show_help` de los errores y desde los enlaces "¿Cómo?" del asistente.
  import { app } from "../app.svelte";
  import { t } from "../i18n/index.svelte";
  import { guide } from "../content";
  import { openExternal } from "../ipc";
  import Icon from "./Icon.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  const topic = $derived(app.help ? guide().topics[app.help] : null);

  $effect(() => {
    if (!dialog) return;
    if (app.help && !dialog.open) dialog.showModal();
    if (!app.help && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} class="drawer" aria-labelledby="help-title" onclose={() => (app.help = null)}>
  {#if topic}
    <header>
      <span class="eyebrow">{t("help.eyebrow")}</span>
      <h2 id="help-title">{topic.title}</h2>
      <button type="button" class="btn ghost small close" aria-label={t("common.close")} onclick={() => (app.help = null)}><Icon name="close" /></button>
    </header>
    <p class="intro">{topic.intro}</p>
    <ol class="steps">
      {#each topic.steps as step, i (i)}
        <li>{step}</li>
      {/each}
    </ol>
    {#if topic.tip}<p class="tip"><Icon name="bulb" size={18} /> {topic.tip}</p>{/if}
    {#if topic.links?.length}
      <div class="links">
        {#each topic.links as l (l.url)}
          <button type="button" class="btn small" onclick={() => openExternal(l.url)}><Icon name="external" size={16} />{l.label}</button>
        {/each}
      </div>
    {/if}
  {/if}
</dialog>

<style>
  .drawer {
    margin: 0 0 0 auto;
    height: 100vh;
    max-height: 100vh;
    width: min(460px, 100vw);
    padding: 1.6rem 1.6rem 2rem;
    border: 0;
    border-left: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    box-shadow: -30px 0 60px -30px rgb(0 0 0 / 0.6);
    animation: slide 320ms var(--ease);
  }
  .drawer::backdrop {
    background: rgb(8 6 4 / 0.55);
    backdrop-filter: blur(2px);
  }
  @keyframes slide {
    from {
      transform: translateX(30px);
      opacity: 0;
    }
  }
  header {
    display: grid;
    gap: 0.4rem;
    position: relative;
    padding-right: 2.5rem;
  }
  .close {
    position: absolute;
    right: -0.5rem;
    top: -0.4rem;
    width: 36px;
    padding: 0;
  }
  .intro {
    margin-top: 1rem;
    color: var(--muted);
  }
  .steps {
    margin: 1.1rem 0 0;
    padding: 0;
    list-style: none;
    counter-reset: s;
    display: grid;
    gap: 0.75rem;
  }
  .steps li {
    counter-increment: s;
    display: grid;
    grid-template-columns: 1.9rem 1fr;
    gap: 0.6rem;
  }
  .steps li::before {
    content: counter(s);
    display: grid;
    place-items: center;
    width: 1.7rem;
    height: 1.7rem;
    border-radius: 50%;
    border: 1px solid var(--accent);
    color: var(--accent);
    font: 500 0.8rem var(--font-mono);
  }
  .tip {
    display: flex;
    gap: 0.5rem;
    margin-top: 1.2rem;
    padding: 0.8rem;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 1.2rem;
  }
</style>
