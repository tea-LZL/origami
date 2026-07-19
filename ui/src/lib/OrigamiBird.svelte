<!-- Origami bird mark, stylized placeholder pending final CI artwork. -->
<script lang="ts">
  let { size = 64 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 64 64"
  fill="none"
  xmlns="http://www.w3.org/2000/svg"
  aria-label="Origami bird"
  role="img"
>
  <!-- back wing (lightest facet) -->
  <polygon points="31,28 10,6 30,38" fill="#93BBFD" />
  <!-- front wing (primary facet) -->
  <polygon points="31,28 54,4 37,40" fill="#2B6CF6" />
  <!-- body -->
  <polygon points="13,46 31,28 45,47 29,54" fill="#1D55E0" />
  <!-- head + beak -->
  <polygon points="45,47 60,41 49,54" fill="#6094FA" />
  <!-- tail (deepest facet) -->
  <polygon points="13,46 3,55 29,54" fill="#1B3A99" />
</svg>
