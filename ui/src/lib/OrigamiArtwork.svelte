<script lang="ts">
  import launchLight from "../../../assets/Origami-bg.png";
  import launchDark from "../../../assets/origami-dark-bg.png";
  import heroLight from "../../../assets/Origami-original.png";
  import heroDark from "../../../assets/Origami-gradient-dark.png";
  import successLight from "../../../assets/Origami-gradient.png";
  import emptyLight from "../../../assets/Origami-outline.png";
  import emptyDark from "../../../assets/Origami-outline-dark.png";
  import setupLight from "../../../assets/Origami-parts.png";
  import caughtUpLight from "../../../assets/Origami-white-bg.png";

  export type ArtworkVariant = "launch" | "hero" | "success" | "empty" | "setup" | "caught-up";
  export type ArtworkTheme = "system" | "light" | "dark" | "ember";

  interface Props {
    variant: ArtworkVariant;
    theme: ArtworkTheme;
    alt?: string;
    className?: string;
    loading?: "eager" | "lazy";
  }

  let {
    variant,
    theme,
    alt = "",
    className = "",
    loading = "lazy",
  }: Props = $props();
  let systemDark = $state(
    typeof window !== "undefined"
      && typeof window.matchMedia === "function"
      && window.matchMedia("(prefers-color-scheme: dark)").matches,
  );

  $effect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => systemDark = media.matches;
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  });

  const dark = $derived(theme === "dark" || theme === "ember" || (theme === "system" && systemDark));
  const source = $derived.by(() => {
    switch (variant) {
      case "launch": return dark ? launchDark : launchLight;
      case "hero": return dark ? heroDark : heroLight;
      case "success": return dark ? heroDark : successLight;
      case "empty": return dark ? emptyDark : emptyLight;
      case "setup": return dark ? heroDark : setupLight;
      case "caught-up": return dark ? launchDark : caughtUpLight;
    }
  });
</script>

<img
  class={`origami-artwork ${variant} ${className}`}
  class:dark
  src={source}
  {alt}
  {loading}
  decoding="async"
/>

<style>
  .origami-artwork {
    display: block;
    max-inline-size: 100%;
    block-size: auto;
    object-fit: contain;
    pointer-events: none;
    user-select: none;
  }
  .origami-artwork.hero:not(.dark),
  .origami-artwork.empty:not(.dark),
  .origami-artwork.setup:not(.dark),
  .origami-artwork.caught-up:not(.dark) { mix-blend-mode: multiply; }
</style>
