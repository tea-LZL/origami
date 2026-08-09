<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    onToggle: (range: boolean) => void;
  }

  let { checked, label, onToggle }: Props = $props();
</script>

<label class="select-control">
  <input
    class="select-input"
    type="checkbox"
    {checked}
    aria-label={label}
    onclick={(event) => {
      event.stopPropagation();
      onToggle(event.shiftKey);
    }}
  />
  <span class="select-paper" aria-hidden="true">
    <svg viewBox="0 0 20 20" focusable="false">
      <path d="m4.75 10.25 3.3 3.4 7.2-7.5" />
    </svg>
  </span>
</label>

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
  .select-input {
    position: absolute;
    inset: 0;
    z-index: 2;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
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
      0 2px 7px rgba(4, 12, 31, 0.15);
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
    box-shadow: inset 0 1px 0 var(--checkbox-glint), 0 3px 9px rgba(4, 12, 31, 0.2);
  }
  .select-input:checked + .select-paper {
    border-color: var(--accent-hover);
    background: linear-gradient(145deg, var(--accent), var(--accent-active));
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.3), 0 3px 10px color-mix(in oklab, var(--accent) 28%, transparent);
  }
  .select-input:checked + .select-paper::before { background: var(--accent-hover); }
  .select-input:checked + .select-paper svg { opacity: 1; }
  .select-input:focus-visible + .select-paper {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  @media (forced-colors: active) {
    .select-input {
      appearance: auto;
      -webkit-appearance: auto;
      inset: 5px;
      width: 18px;
      height: 18px;
      opacity: 1;
    }
    .select-paper { display: none; }
  }
</style>
