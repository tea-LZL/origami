<script lang="ts">
  import origamiBird from "../../../assets/origami-bird.svg";

  let { size = 64 }: { size?: number } = $props();
</script>

<img
  src={origamiBird}
  width={size}
  height={size}
  alt="Origami bird"
/>
