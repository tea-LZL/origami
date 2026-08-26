<script lang="ts">
  import origamiBird from "../../../assets/origami-bird.svg";

  let { size = 64, alt = "Origami bird" }: { size?: number; alt?: string } = $props();
</script>

<img
  src={origamiBird}
  width={size}
  height={size}
  {alt}
/>
