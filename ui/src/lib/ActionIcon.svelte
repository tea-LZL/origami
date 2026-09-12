<script lang="ts">
  type ActionIconName =
    | "reply"
    | "reply-all"
    | "forward"
    | "archive"
    | "trash"
    | "junk"
    | "star";

  let { name, filled = false }: { name: ActionIconName; filled?: boolean } = $props();
</script>

<svg
  viewBox="0 0 16 16"
  width="16"
  height="16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "reply"}
    <path d="M6.5 3.5 3 7l3.5 3.5" />
    <path d="M3 7h6.5a3.5 3.5 0 0 1 3.5 3.5V12" />
  {:else if name === "reply-all"}
    <path d="M5.5 3.5 2 7l3.5 3.5" />
    <path d="M9.5 3.5 6 7l3.5 3.5" />
    <path d="M6 7h5a3.5 3.5 0 0 1 3.5 3.5V12" />
  {:else if name === "forward"}
    <path d="M9.5 3.5 13 7l-3.5 3.5" />
    <path d="M13 7H6.5A3.5 3.5 0 0 0 3 10.5V12" />
  {:else if name === "archive"}
    <path d="M3 2.5h10a.5.5 0 0 1 .5.5v2a.5.5 0 0 1-.5.5H3a.5.5 0 0 1-.5-.5V3a.5.5 0 0 1 .5-.5Z" />
    <path d="M3.5 5.5v6a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-6" />
    <path d="M6.5 8.5h3" />
  {:else if name === "trash"}
    <path d="M2.5 4.5h11" />
    <path d="M6.5 4.5V3a.5.5 0 0 1 .5-.5h2a.5.5 0 0 1 .5.5v1.5" />
    <path d="M4.2 4.5l.7 8a1 1 0 0 0 1 .9h4.2a1 1 0 0 0 1-.9l.7-8" />
    <path d="M6.8 7v4M9.2 7v4" />
  {:else if name === "junk"}
    <path d="M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11Z" />
    <path d="M4.3 4.3l7.4 7.4" />
  {:else if name === "star"}
    <path
      d="M8 2.4l1.35 3.94 4.17.07-3.33 2.5 1.22 3.98L8 10.5l-3.41 2.39 1.22-3.98-3.33-2.5 4.17-.07Z"
      fill={filled ? "currentColor" : "none"}
    />
  {/if}
</svg>
