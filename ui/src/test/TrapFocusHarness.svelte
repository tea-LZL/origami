<script lang="ts">
  import { trapFocus } from "../lib/trapFocus";

  let { onEscape }: { onEscape?: () => void } = $props();
</script>

<div use:trapFocus={{ onEscape }} role="dialog" aria-modal="true" aria-label="Trap">
  <button type="button">One</button>
  <button type="button">Two</button>
</div>
