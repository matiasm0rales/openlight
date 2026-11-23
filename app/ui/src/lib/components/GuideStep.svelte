<script lang="ts">
  // Pasos 3–5: la guía del portal de Tuya sale del contenido editable (src/content/guide).
  import type { Snippet } from "svelte";
  import type { HelpTopic } from "../bindings";
  import { guide, imageUrl } from "../content";
  import { openExternal } from "../ipc";
  import { app } from "../app.svelte";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let {
    section,
    help,
    confirmLabel,
    confirmed = $bindable(false),
    children,
    aside,
  }: {
    section: "tuya_account" | "cloud_project" | "link_app";
    help: HelpTopic;
    confirmLabel: string;
    confirmed: boolean;
    children?: Snippet;
    aside?: Snippet;
  } = $props();

  const g = $derived(guide().wizard[section]);
  const img = $derived(imageUrl(g.image));
</script>

<div class="guide">
  <div class="col">
    <p>{g.lead}</p>
    <ol class="steps">
      {#each g.steps as s, i (i)}<li>{s}</li>{/each}
    </ol>
    {#if g.phone_steps}
      <div class="phone">
        <p class="eyebrow"><Icon name="phone" size={14} /> {t("onboarding.on_your_phone")}</p>
        <ol class="steps">
          {#each g.phone_steps as s, i (i)}<li>{s}</li>{/each}
        </ol>
      </div>
    {/if}
    {#if g.note}<p class="note"><Icon name="bulb" size={18} /> <span>{g.note}</span></p>{/if}
    {@render children?.()}
    <div class="actions">
      {#if g.link}
        <button type="button" class="btn" onclick={() => openExternal(g.link!.url)}><Icon name="external" size={16} />{g.link.label}</button>
      {/if}
      <button type="button" class="btn ghost" onclick={() => (app.help = help)}><Icon name="help" size={16} />{t("onboarding.how")}</button>
    </div>
    <label class="confirm" class:on={confirmed}>
      <input type="checkbox" bind:checked={confirmed} />
      <span>{confirmLabel}</span>
    </label>
  </div>
  <figure class="art">
    {#if img}<img src={img} alt={t(`onboarding.${section}.image_alt`)} />{/if}
    {@render aside?.()}
  </figure>
</div>

<style>
  .guide {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr);
    gap: 2rem;
    align-items: start;
  }
  .col {
    display: grid;
    gap: 1.1rem;
  }
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: s;
    display: grid;
    gap: 0.7rem;
  }
  .steps li {
    counter-increment: s;
    display: grid;
    grid-template-columns: 2rem 1fr;
    gap: 0.5rem;
  }
  .steps li::before {
    content: counter(s, decimal-leading-zero);
    font: 500 0.8rem/1.9 var(--font-mono);
    color: var(--accent);
  }
  .phone {
    display: grid;
    gap: 0.6rem;
    padding: 0.9rem 1rem;
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .note {
    display: flex;
    gap: 0.6rem;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .confirm {
    display: flex;
    gap: 0.7rem;
    align-items: center;
    padding: 0.9rem 1rem;
    border-radius: var(--radius);
    border: 1px dashed var(--line-strong);
    font-weight: 700;
    cursor: pointer;
  }
  .confirm.on {
    border-style: solid;
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .art {
    margin: 0;
    display: grid;
    gap: 1rem;
    position: sticky;
    top: 1.5rem;
  }
  .art img {
    width: 100%;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: var(--bg-deep);
  }
  @media (max-width: 900px) {
    .guide {
      grid-template-columns: 1fr;
    }
    .art {
      position: static;
    }
  }
</style>
