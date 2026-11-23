<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { wizard } from "../../lib/wizard.svelte";
  import GuideStep from "../../lib/components/GuideStep.svelte";

  let { valid = $bindable(), completed }: { valid: boolean; completed: boolean } = $props();
  let confirmed = $state(wizard.confirmed.tuya_account ?? untrack(() => completed));
  $effect(() => {
    wizard.confirmed.tuya_account = confirmed;
    valid = confirmed;
  });
</script>

<GuideStep section="tuya_account" help="tuya_account" confirmLabel={t("onboarding.tuya_account.confirm")} bind:confirmed />
