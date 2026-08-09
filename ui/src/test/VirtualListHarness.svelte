<script lang="ts">
  import VirtualList from "../lib/VirtualList.svelte";

  const items = ["first message", "second message"];
</script>

<VirtualList
  {items}
  itemHeight={40}
  selectedId={(item) => item}
>
  {#snippet render(item: string)}
    <span>{item}</span>
  {/snippet}
</VirtualList>
