<script lang="ts">
  import { onDestroy } from "svelte";

  interface Props {
    value: number;
    min: number;
    max: number;
    label: string;
    onResize: (value: number) => void;
    onResizeEnd?: () => void;
  }

  let { value, min, max, label, onResize, onResizeEnd }: Props = $props();
  let stopDragging: (() => void) | null = null;

  function clamp(next: number): number {
    return Math.min(max, Math.max(min, Math.round(next)));
  }

  function startDragging(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();

    const startX = event.clientX;
    const startValue = value;
    const move = (moveEvent: PointerEvent) => {
      onResize(clamp(startValue + moveEvent.clientX - startX));
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      stopDragging = null;
      onResizeEnd?.();
    };

    stopDragging?.();
    stopDragging = stop;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }

  function adjust(event: KeyboardEvent) {
    const step = event.shiftKey ? 48 : 16;
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      onResize(clamp(value - step));
      onResizeEnd?.();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      onResize(clamp(value + step));
      onResizeEnd?.();
    } else if (event.key === "Home") {
      event.preventDefault();
      onResize(min);
      onResizeEnd?.();
    } else if (event.key === "End") {
      event.preventDefault();
      onResize(max);
      onResizeEnd?.();
    }
  }

  onDestroy(() => stopDragging?.());
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="splitter"
  role="separator"
  aria-label={label}
  aria-orientation="vertical"
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={Math.round(value)}
  tabindex="0"
  onpointerdown={startDragging}
  onkeydown={adjust}
></div>

<style>
  .splitter {
    position: relative;
    z-index: 2;
    flex: 0 0 9px;
    width: 9px;
    margin-inline: -4px;
    cursor: col-resize;
    touch-action: none;
  }

  .splitter::after {
    content: "";
    position: absolute;
    inset: 0 4px;
    background: transparent;
    transition: background var(--transition-fast);
  }

  .splitter:hover::after,
  .splitter:focus-visible::after {
    background: var(--accent);
  }
</style>
