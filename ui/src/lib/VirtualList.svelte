<!-- Simple windowed list with a fixed row height. Renders only the
     visible window of items plus a small overscan, keeping 10k-message
     folders at a steady 60fps. -->

<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  interface Props {
    items: T[];
    itemHeight: number;
    render: Snippet<[T, number]>;
    onSelect?: (item: T, event: MouseEvent | KeyboardEvent) => void;
    selectedId?: (item: T) => string | null;
    activeId?: string | null;
    isSelected?: (item: T) => boolean;
    onEndReached?: () => void;
  }

  let {
    items,
    itemHeight,
    render,
    onSelect,
    selectedId,
    activeId = null,
    isSelected,
    onEndReached,
  }: Props = $props();

  let scroller: HTMLDivElement | null = $state(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let scrollFrame: number | null = null;

  const overscan = 6;

  const total = $derived(items.length);
  const startIndex = $derived(
    Math.max(0, Math.floor(scrollTop / itemHeight) - overscan),
  );
  const endIndex = $derived(
    Math.min(
      total,
      Math.ceil((scrollTop + viewportHeight) / itemHeight) + overscan,
    ),
  );
  const visible = $derived(items.slice(startIndex, endIndex));
  const offsetY = $derived(startIndex * itemHeight);
  const totalHeight = $derived(total * itemHeight);

  function onScroll() {
    if (!scroller) return;
    if (scrollFrame !== null) return;
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null;
      if (!scroller) return;
      scrollTop = scroller.scrollTop;
      if (scrollTop + scroller.clientHeight >= scroller.scrollHeight - itemHeight * 4) {
        onEndReached?.();
      }
    });
  }

  function measureViewport(node: HTMLDivElement) {
    scroller = node;
    const update = () => viewportHeight = node.clientHeight;
    const observer = new ResizeObserver(update);
    observer.observe(node);
    update();
    return {
      destroy() {
        observer.disconnect();
        if (scrollFrame !== null) cancelAnimationFrame(scrollFrame);
        scrollFrame = null;
        if (scroller === node) scroller = null;
      },
    };
  }

  function key(item: T): string | number {
    if (selectedId) {
      const s = selectedId(item);
      if (s != null) return s;
    }
    try {
      return JSON.stringify(item);
    } catch {
      return Math.random();
    }
  }

  function onRowKeydown(event: KeyboardEvent, item: T) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    onSelect?.(item, event);
  }
</script>

<div
  class="virtual"
  data-navigation="message-list"
  use:measureViewport
  onscroll={onScroll}
  role="listbox"
  aria-label="Messages"
  aria-keyshortcuts="J K Enter Space"
  aria-multiselectable="true"
  tabindex={activeId == null ? 0 : -1}
>
  <div class="spacer" style:height="{totalHeight}px">
    <div class="window" style:transform="translateY({offsetY}px)">
      {#each visible as item, i (key(item))}
        <div
          class="row"
          class:active={activeId != null && selectedId?.(item) === activeId}
          class:checked={isSelected?.(item) ?? false}
          data-item-id={selectedId?.(item) ?? undefined}
          style:height="{itemHeight}px"
          role="option"
          aria-selected={isSelected?.(item) ?? false}
          tabindex={activeId != null && selectedId?.(item) === activeId ? 0 : -1}
          onclick={(event) => onSelect?.(item, event)}
          onkeydown={(event) => onRowKeydown(event, item)}
        >
          {@render render(item, startIndex + i)}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .virtual {
    flex: 1 1 0;
    width: 100%;
    height: auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--bg);
  }
  .spacer {
    position: relative;
    width: 100%;
  }
  .window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
  .row {
    position: relative;
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 82%, var(--paper-highlight));
    padding: 8px 12px;
    color: var(--fg);
    cursor: pointer;
    transition: background-color var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .row:hover {
    background: color-mix(in oklab, var(--bg-sunken) 84%, var(--accent));
  }
  .row.active {
    background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised));
    color: var(--fg);
  }
  .row.active::before {
    content: "";
    position: absolute;
    top: 9px;
    bottom: 9px;
    left: 5px;
    width: 3px;
    border-radius: 999px;
    background: var(--accent);
    pointer-events: none;
  }
  .row:focus-visible {
    z-index: 1;
    outline-offset: -2px;
  }
  .row.checked:not(.active) {
    background: color-mix(in oklab, var(--accent) 8%, transparent);
  }
</style>
