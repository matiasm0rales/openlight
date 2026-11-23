<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import GuideStep from "../../lib/components/GuideStep.svelte";

  let { valid = $bindable(), completed }: { valid: boolean; completed: boolean } = $props();
  let confirmed = $state(wizard.confirmed.link_app ?? untrack(() => completed));
  $effect(() => {
    wizard.confirmed.link_app = confirmed;
    valid = confirmed;
  });
</script>

<GuideStep section="link_app" help="link_smart_life" confirmLabel={t("onboarding.link_app.confirm")} bind:confirmed />
