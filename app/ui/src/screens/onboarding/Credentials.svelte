<script lang="ts">
  // Paso 6: Access ID y Access Secret, "Probar" y errores humanos con arreglo concreto.
  // Si el Data Center está mal, el backend sugiere el correcto y se ofrece con un clic.
  import type { CloudAccount, Region, UserFacingError } from "../../lib/bindings";
  import { attempt, commands } from "../../lib/ipc";
  import { app } from "../../lib/app.svelte";
  import { guide } from "../../lib/content";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import ErrorCard from "../../lib/components/ErrorCard.svelte";
  import Icon from "../../lib/components/Icon.svelte";

  let { valid = $bindable() }: { valid: boolean } = $props();

  const REGIONS: Region[] = ["us", "us_east", "eu", "eu_west", "india", "china", "singapore"];
  let accessId = $state("");
  let secret = $state("");
  let region = $state<Region>(wizard.region ?? "us");
  let reveal = $state(false);
  let testing = $state(false);
  let account = $state<CloudAccount | null>(null);
  let error = $state<UserFacingError | null>(null);
  const saved = $derived(Boolean(app.onboarding?.has_credentials) && !accessId && !secret);

  $effect(() => {
    valid = account !== null || saved;
  });

  // Validación en vivo del formato (antes de preguntarle a Tuya).
  const secretHint = $derived(secret.trim().length > 0 && secret.trim().length !== 32 ? t("onboarding.credentials.secret_length", { n: secret.trim().length }) : "");
  const idHint = $derived(accessId.trim().length > 0 && accessId.trim().length < 10 ? t("onboarding.credentials.id_short") : "");

  async function test(r: Region = region) {
    region = r;
    wizard.region = r;
    testing = true;
    error = null;
    account = null;
    const res = await attempt(() => commands.cloudValidate({ access_id: accessId.trim(), access_secret: secret.trim(), region: r }));
    testing = false;
    if (res.ok) {
      account = res.value;
      if (app.onboarding) app.onboarding.has_credentials = true;
    } else error = res.error;
  }

  function edited() {
    account = null;
  }

  const suggested = $derived((error?.code === "cloud_wrong_region" ? error.params.region : undefined) as Region | undefined);
</script>

<div class="creds">
  <p class="muted">{guide().wizard.credentials.lead}</p>
  <form class="card form" onsubmit={(e) => { e.preventDefault(); void test(); }} novalidate>
    <div class="field">
      <label for="access-id">Access ID</label>
      <input id="access-id" type="text" bind:value={accessId} oninput={edited} autocomplete="off" spellcheck="false" aria-invalid={Boolean(idHint) || error?.code === "cloud_invalid_token"} aria-describedby="access-id-hint" />
      <p id="access-id-hint" class="hint" class:bad={idHint}>{idHint || t("onboarding.credentials.id_help")}</p>
    </div>
    <div class="field">
      <label for="access-secret">Access Secret</label>
      <div class="secret">
        <input id="access-secret" type={reveal ? "text" : "password"} bind:value={secret} oninput={edited} autocomplete="off" spellcheck="false" aria-invalid={Boolean(secretHint) || error?.code === "cloud_invalid_signature"} aria-describedby="access-secret-hint" />
        <button type="button" class="btn ghost small" aria-pressed={reveal} onclick={() => (reveal = !reveal)}><Icon name="eye" size={16} />{reveal ? t("onboarding.credentials.hide") : t("onboarding.credentials.show")}</button>
      </div>
      <p id="access-secret-hint" class="hint" class:bad={secretHint}>{secretHint || t("onboarding.credentials.secret_help")}</p>
    </div>
    <div class="field">
      <label for="region">{t("onboarding.credentials.region")}</label>
      <select id="region" bind:value={region} onchange={edited}>
        {#each REGIONS as r (r)}<option value={r}>{t(`region.${r}`)}</option>{/each}
      </select>
    </div>
    <div class="actions">
      <button type="submit" class="btn primary" disabled={testing || !accessId.trim() || !secret.trim()}>
        {#if testing}<span class="spinner" aria-hidden="true"></span>{t("onboarding.credentials.testing")}{:else}<Icon name="key" size={16} />{t("onboarding.credentials.test")}{/if}
      </button>
      <p class="muted small">{guide().wizard.credentials.note}</p>
    </div>
  </form>

  <div aria-live="polite">
    {#if account}
      <p class="ok" role="status"><Icon name="check" size={18} /> {t("onboarding.credentials.ok", { region: account.region })}</p>
    {:else if saved}
      <p class="ok" role="status"><Icon name="check" size={18} /> {t("onboarding.credentials.saved")}</p>
    {/if}
  </div>
  {#if error}
    <ErrorCard
      {error}
      retry={() => test(suggested ?? region)}
      retryLabel={suggested ? t("fix.use_region", { region: suggested }) : undefined}
    />
  {/if}
</div>

<style>
  .creds {
    display: grid;
    gap: 1.2rem;
    max-width: 640px;
  }
  .form {
    display: grid;
    gap: 1rem;
    padding: 1.3rem;
  }
  .secret {
    display: flex;
    gap: 0.5rem;
  }
  .secret input {
    font-family: var(--font-mono);
  }
  #access-id {
    font-family: var(--font-mono);
  }
  .hint {
    font-size: 0.84rem;
    color: var(--faint);
  }
  .hint.bad {
    color: var(--danger);
  }
  .actions {
    display: flex;
    gap: 1rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .small {
    font-size: 0.84rem;
    flex: 1 1 220px;
  }
  .ok {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    padding: 0.8rem 1rem;
    border-radius: var(--radius-sm);
    background: var(--ok-soft);
    color: var(--ok);
    font-weight: 700;
  }
</style>
