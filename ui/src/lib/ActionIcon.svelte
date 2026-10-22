<script lang="ts">
  type ActionIconName =
    | "reply"
    | "reply-all"
    | "forward"
    | "archive"
    | "trash"
    | "junk"
    | "star"
    | "bold"
    | "italic"
    | "underline"
    | "heading"
    | "list"
    | "ordered-list"
    | "quote"
    | "code"
    | "link"
    | "undo"
    | "redo"
    | "image"
    | "paperclip"
    | "minimize";

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
  {:else if name === "bold"}
    <path d="M5 3h4a2.5 2.5 0 0 1 0 5H5Z" />
    <path d="M5 8h4.5a2.5 2.5 0 0 1 0 5H5Z" />
  {:else if name === "italic"}
    <path d="M6.5 3h5M4.5 13h5M9.5 3l-3 10" />
  {:else if name === "underline"}
    <path d="M4.5 3v4.5a3.5 3.5 0 0 0 7 0V3" />
    <path d="M4 13h8" />
  {:else if name === "heading"}
    <path d="M4 3v10M12 3v10M4 8h8" />
  {:else if name === "list"}
    <path d="M6.5 4h7M6.5 8h7M6.5 12h7" />
    <circle cx="3.4" cy="4" r=".8" fill="currentColor" stroke="none" />
    <circle cx="3.4" cy="8" r=".8" fill="currentColor" stroke="none" />
    <circle cx="3.4" cy="12" r=".8" fill="currentColor" stroke="none" />
  {:else if name === "ordered-list"}
    <path d="M6.5 4h7M6.5 8h7M6.5 12h7" />
    <path d="M2.7 3.1 3.7 2.5V5M2.7 7.2c.3-.5 1.4-.5 1.4.2 0 .5-1.4 1.2-1.4 1.6h1.5M2.7 11.1h1.2c.6 0 .6 1 0 1 .6 0 .6 1 0 1H2.7" />
  {:else if name === "quote"}
    <path d="M3 9V7.2c0-1.8 1.1-3 2.8-3.4M9.2 9V7.2c0-1.8 1.1-3 2.8-3.4" />
    <path d="M3 9h2.3v2.3H3zM9.2 9h2.3v2.3H9.2z" />
  {:else if name === "code"}
    <path d="M5.5 4 2.5 8l3 4M10.5 4l3 4-3 4" />
  {:else if name === "link"}
    <path d="M6.6 9.4a2.3 2.3 0 0 0 3.3 0l1.8-1.8a2.3 2.3 0 0 0-3.3-3.3l-1 1" />
    <path d="M9.4 6.6a2.3 2.3 0 0 0-3.3 0L4.3 8.4a2.3 2.3 0 0 0 3.3 3.3l1-1" />
  {:else if name === "undo"}
    <path d="M4.5 5.5H10a3.5 3.5 0 0 1 0 7H7" />
    <path d="M6.8 3.2 4.3 5.7l2.5 2.5" />
  {:else if name === "redo"}
    <path d="M11.5 5.5H6a3.5 3.5 0 0 0 0 7h3" />
    <path d="M9.2 3.2l2.5 2.5-2.5 2.5" />
  {:else if name === "image"}
    <rect x="2.5" y="3" width="11" height="10" rx="1.2" />
    <circle cx="6" cy="6.2" r="1" />
    <path d="m3.2 11.5 3.1-3 2.1 2 1.7-1.5 2.7 2.5" />
  {:else if name === "paperclip"}
    <path d="m10.7 7.3-3.4 3.4a2.2 2.2 0 0 1-3.1-3.1l4.2-4.2a1.6 1.6 0 0 1 2.3 2.3L6.5 9.9a.9.9 0 0 1-1.3-1.3l3.3-3.3" />
  {:else if name === "minimize"}
    <path d="M4 8h8" />
  {/if}
</svg>
