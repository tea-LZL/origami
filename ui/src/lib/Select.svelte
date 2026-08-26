<script lang="ts">
  export interface SelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value?: string | null;
    options: SelectOption[];
    ariaLabel: string;
    placeholder?: string;
    disabled?: boolean;
    compact?: boolean;
    onValueChange?: (value: string) => void;
  }

  let {
    value = $bindable(null),
    options,
    ariaLabel,
    placeholder = "Select…",
    disabled = false,
    compact = false,
    onValueChange,
  }: Props = $props();

  const uid = $props.id();
  const menuId = `${uid}-menu`;
  let trigger: HTMLButtonElement | null = $state(null);
  let menu: HTMLDivElement | null = $state(null);
  let open = $state(false);
  let activeIndex = $state(-1);
  let typeahead = "";
  let typeaheadTimer: ReturnType<typeof setTimeout> | null = null;

  const selectedIndex = $derived(options.findIndex((option) => option.value === value));
  const selectedLabel = $derived(selectedIndex >= 0 ? options[selectedIndex].label : placeholder);

  function selectableIndexes(): number[] {
    return options.flatMap((option, index) => option.disabled ? [] : [index]);
  }

  function positionMenu() {
    if (!trigger || !menu || !menu.matches(":popover-open")) return;
    const rect = trigger.getBoundingClientRect();
    const gutter = 8;
    const width = Math.min(Math.max(rect.width, 180), window.innerWidth - gutter * 2);
    menu.style.width = `${width}px`;
    const height = Math.min(menu.scrollHeight, 280);
    const below = window.innerHeight - rect.bottom - gutter;
    const top = below >= Math.min(height, 160)
      ? rect.bottom + 6
      : Math.max(gutter, rect.top - height - 6);
    const left = Math.min(Math.max(gutter, rect.left), window.innerWidth - width - gutter);
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
  }

  function focusActive() {
    if (!menu || activeIndex < 0) return;
    menu.querySelector<HTMLButtonElement>(`[data-option-index="${activeIndex}"]`)?.focus();
  }

  function openMenu(preferLast = false) {
    if (disabled || !menu || menu.matches(":popover-open")) return;
    const indexes = selectableIndexes();
    if (indexes.length === 0) return;
    activeIndex = selectedIndex >= 0 && !options[selectedIndex].disabled
      ? selectedIndex
      : preferLast ? indexes[indexes.length - 1] : indexes[0];
    menu.showPopover();
    open = true;
    requestAnimationFrame(() => {
      positionMenu();
      focusActive();
    });
  }

  function closeMenu(restoreFocus = true) {
    if (menu?.matches(":popover-open")) menu.hidePopover();
    open = false;
    if (restoreFocus) requestAnimationFrame(() => trigger?.focus());
  }

  function toggleMenu() {
    if (open) closeMenu();
    else openMenu();
  }

  function choose(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    value = option.value;
    onValueChange?.(option.value);
    closeMenu();
  }

  function moveActive(direction: 1 | -1) {
    const indexes = selectableIndexes();
    if (indexes.length === 0) return;
    const current = indexes.indexOf(activeIndex);
    const next = current < 0
      ? direction > 0 ? 0 : indexes.length - 1
      : (current + direction + indexes.length) % indexes.length;
    activeIndex = indexes[next];
    focusActive();
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      openMenu(event.key === "ArrowUp");
    }
  }

  function onMenuKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      moveActive(event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const indexes = selectableIndexes();
      activeIndex = event.key === "Home" ? indexes[0] : indexes[indexes.length - 1];
      focusActive();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(activeIndex);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu();
      return;
    }
    if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
      typeahead += event.key.toLocaleLowerCase();
      if (typeaheadTimer) clearTimeout(typeaheadTimer);
      typeaheadTimer = setTimeout(() => typeahead = "", 500);
      const match = options.findIndex((option) =>
        !option.disabled && option.label.toLocaleLowerCase().startsWith(typeahead)
      );
      if (match >= 0) {
        activeIndex = match;
        focusActive();
      }
    }
  }
</script>

<svelte:window onresize={positionMenu} />

<button
  bind:this={trigger}
  type="button"
  class="select-trigger"
  class:compact
  class:placeholder={selectedIndex < 0}
  aria-label={ariaLabel}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls={menuId}
  popovertarget={menuId}
  popovertargetaction="toggle"
  {disabled}
  onclick={(event) => {
    event.preventDefault();
    toggleMenu();
  }}
  onkeydown={onTriggerKeydown}
>
  <span>{selectedLabel}</span>
  <svg viewBox="0 0 12 12" aria-hidden="true">
    <path d="m2.5 4.25 3.5 3.5 3.5-3.5" />
  </svg>
</button>

<div
  bind:this={menu}
  id={menuId}
  class="select-menu"
  popover="auto"
  role="listbox"
  tabindex="-1"
  aria-label={ariaLabel}
  onkeydown={onMenuKeydown}
  ontoggle={(event) => {
    open = (event as ToggleEvent).newState === "open";
    if (!open) activeIndex = -1;
  }}
>
  {#each options as option, index (option.value)}
    <button
      type="button"
      role="option"
      aria-selected={option.value === value}
      data-option-index={index}
      class:active={index === activeIndex}
      class:selected={option.value === value}
      disabled={option.disabled}
      onclick={() => choose(index)}
      onfocus={() => activeIndex = index}
    >
      <span>{option.label}</span>
      {#if option.value === value}
        <svg viewBox="0 0 14 14" aria-hidden="true"><path d="m2.5 7.5 3 3 6-7" /></svg>
      {/if}
    </button>
  {/each}
</div>

<style>
  .select-trigger {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    width: 100%;
    min-height: 36px;
    padding: 7px 34px 7px 10px;
    border: 1px solid var(--select-edge);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    background: linear-gradient(145deg, var(--select-face), var(--select-face-shade));
    color: var(--fg);
    box-shadow: inset 0 1px 0 var(--select-glint), 0 1px 3px rgba(35, 31, 25, 0.08);
    text-align: left;
  }
  .select-trigger::after {
    content: "";
    position: absolute;
    top: 0;
    right: 0;
    width: 13px;
    height: 13px;
    border-radius: 0 var(--radius-sm) 0 0;
    background: linear-gradient(225deg, var(--select-fold) 49%, transparent 51%);
    pointer-events: none;
  }
  .select-trigger:hover:not(:disabled),
  .select-trigger[aria-expanded="true"] {
    border-color: var(--accent);
    box-shadow: inset 0 1px 0 var(--select-glint), 0 0 0 2px color-mix(in oklab, var(--accent) 11%, transparent);
  }
  .select-trigger span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .select-trigger svg {
    position: absolute;
    right: 10px;
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform var(--transition-fast);
  }
  .select-trigger[aria-expanded="true"] > svg { transform: rotate(180deg); }
  .select-trigger.placeholder { color: var(--fg-subtle); }
  .select-trigger.compact { width: auto; min-width: 76px; max-width: 130px; min-height: 28px; padding: 3px 28px 3px 7px; font-size: 10px; }
  .select-trigger.compact svg { right: 8px; width: 10px; height: 10px; }
  .select-trigger:disabled { cursor: not-allowed; opacity: 0.58; }

  .select-menu {
    position: fixed;
    inset: auto;
    max-height: min(280px, calc(100vh - 16px));
    margin: 0;
    padding: 6px;
    overflow-y: auto;
    border: 1px solid var(--select-edge);
    border-radius: var(--radius-md);
    background: var(--select-panel);
    color: var(--fg);
    box-shadow: var(--shadow-float), inset 0 1px 0 var(--select-glint);
  }
  .select-menu:popover-open { animation: select-in var(--transition-fast) both; }
  .select-menu button {
    width: 100%;
    min-height: 32px;
    padding: 6px 30px 6px 9px;
    border-radius: var(--radius-sm);
    position: relative;
    display: flex;
    align-items: center;
    color: var(--fg-muted);
    text-align: left;
  }
  .select-menu button:hover:not(:disabled),
  .select-menu button.active { background: var(--select-option-hover); color: var(--fg); }
  .select-menu button.selected { background: var(--select-option-selected); color: var(--accent); font-weight: 700; }
  .select-menu button:disabled { color: var(--fg-subtle); cursor: not-allowed; opacity: 0.6; }
  .select-menu button svg {
    position: absolute;
    right: 9px;
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  @keyframes select-in {
    from { opacity: 0; transform: translateY(-3px) scale(0.985); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
  @media (forced-colors: active) {
    .select-trigger, .select-menu { border-color: ButtonText; background: Field; color: FieldText; box-shadow: none; }
    .select-trigger::after { content: none; }
    .select-menu button.selected, .select-menu button.active { background: Highlight; color: HighlightText; }
  }
</style>
