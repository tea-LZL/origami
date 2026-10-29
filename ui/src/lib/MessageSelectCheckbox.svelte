<script lang="ts">
  interface Props {
    checked: boolean;
  }

  let { checked }: Props = $props();
</script>

<!-- Presentational check affordance. The row is the listbox option and
     exposes aria-selected for this checked state; focus marks the row that is
     open in the reader, and the Space handler toggles the check. ThreadList
     routes clicks on this box to selection instead of opening the message. -->
<span class="select-control" class:checked>
  <span class="select-paper">
    <svg viewBox="0 0 20 20" focusable="false">
      <path d="m4.75 10.25 3.3 3.4 7.2-7.5" />
    </svg>
  </span>
</span>

<style>
  .select-control {
    position: absolute;
    left: -3px;
    top: -3px;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .select-paper {
    position: relative;
    width: 21px;
    height: 21px;
    overflow: hidden;
    border: 1px solid var(--checkbox-edge);
    border-radius: 6px;
    background: linear-gradient(145deg, var(--checkbox-paper), var(--checkbox-paper-shade));
    box-shadow:
      inset 0 1px 0 var(--checkbox-glint),
      0 2px 5px rgba(49, 45, 38, 0.14);
    pointer-events: none;
    transition: border-color var(--transition-fast), background-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .select-paper::before {
    content: "";
    position: absolute;
    top: 0;
    right: 0;
    width: 8px;
    height: 8px;
    background: var(--checkbox-fold);
    clip-path: polygon(100% 0, 100% 100%, 0 0);
  }
  .select-paper::after {
    content: "";
    position: absolute;
    top: 7px;
    right: -1px;
    width: 10px;
    height: 1px;
    background: color-mix(in oklab, var(--checkbox-edge) 78%, transparent);
    transform: rotate(45deg);
    transform-origin: right center;
  }
  svg {
    position: absolute;
    inset: 0;
    z-index: 1;
    width: 100%;
    height: 100%;
    fill: none;
    stroke: var(--accent-fg);
    stroke-width: 2.25;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0;
  }
  .select-control:hover .select-paper {
    border-color: var(--accent);
    box-shadow: inset 0 1px 0 var(--checkbox-glint), 0 3px 8px rgba(49, 45, 38, 0.2);
  }
  .select-control.checked .select-paper {
    border-color: var(--accent-hover);
    background: linear-gradient(145deg, var(--accent), var(--accent-active));
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.3), 0 3px 10px color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .select-control.checked .select-paper::before { background: var(--accent-hover); }
  .select-control.checked svg { opacity: 1; }
  @media (forced-colors: active) {
    .select-paper { border-color: CanvasText; forced-color-adjust: none; }
    .select-control.checked .select-paper { background: Highlight; }
    .select-control.checked svg { stroke: HighlightText; opacity: 1; }
  }
</style>
