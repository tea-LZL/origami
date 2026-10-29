<script lang="ts">
  import { paletteItems, type PaletteItem } from "./commands";
  import { app } from "./stores.svelte";
  import { trapFocus } from "./trapFocus";

  const GROUPS = ["Triage", "Compose", "Folders", "Help"] as const;
  const uid = $props.id();
  const listId = `command-palette-${uid}`;

  let query = $state("");
  let highlight = $state(0);

  const items = $derived(paletteItems(query));
  const grouped = $derived.by(() => {
    if (query.trim()) {
      return [{ group: null as string | null, rows: items.map((item, index) => ({ item, index })) }];
    }
    return GROUPS.flatMap((group) => {
      const rows = items
        .map((item, index) => ({ item, index }))
        .filter(({ item }) => item.group === group);
      return rows.length ? [{ group, rows }] : [];
    });
  });
  const activeItem = $derived(items[highlight] ?? null);
  const activeId = $derived(activeItem ? optionId(activeItem) : undefined);

  $effect(() => {
    if (app.value.commandPaletteOpen) query = "";
  });

  $effect(() => {
    const first = items.findIndex((item) => item.enabled);
    highlight = first >= 0 ? first : 0;
  });

  function optionId(item: PaletteItem): string {
    return `${listId}-${item.id.replaceAll(/[^a-zA-Z0-9_-]/g, "-")}`;
  }

  function close() {
    app.value.commandPaletteOpen = false;
    query = "";
  }

  function onOverlay(event: MouseEvent) {
    if (event.target === event.currentTarget) close();
  }

  function enabledIndexes(): number[] {
    return items.flatMap((item, index) => item.enabled ? [index] : []);
  }

  function move(delta: number) {
    const indexes = enabledIndexes();
    if (indexes.length === 0) return;
    const current = indexes.indexOf(highlight);
    const from = current >= 0 ? current : (delta > 0 ? -1 : 0);
    const next = indexes[(from + delta + indexes.length) % indexes.length];
    if (next != null) highlight = next;
  }

  function focusAfterRun() {
    requestAnimationFrame(() => {
      const id = app.value.selectedEnvelope?.id;
      const target = id
        ? document.querySelector<HTMLElement>(`[data-item-id="${CSS.escape(id)}"]`)
        : document.querySelector<HTMLElement>('[data-navigation="message-list"]');
      target?.focus();
    });
  }

  function runItem(item: PaletteItem) {
    if (!item.enabled) return;
    void item.run();
    close();
    focusAfterRun();
  }

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
      return;
    }
    if (
      query === ""
      && !event.shiftKey
      && !event.ctrlKey
      && !event.metaKey
      && (event.key === "j" || event.key === "k")
    ) {
      event.preventDefault();
      move(event.key === "j" ? 1 : -1);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (activeItem?.enabled) runItem(activeItem);
    }
  }
</script>

{#if app.value.commandPaletteOpen}
  <div
    class="overlay"
    role="presentation"
    use:trapFocus={{ onEscape: close }}
    onclick={onOverlay}
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <input
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={activeId}
        aria-autocomplete="list"
        aria-label="Command"
        placeholder="Type a command or folder"
        bind:value={query}
        onkeydown={onSearchKey}
      />
      <div id={listId} class="list" role="listbox">
        {#if items.length === 0 && query.trim()}
          <p class="empty">No matching commands</p>
        {:else}
          {#each grouped as section}
            {#if section.group}
              <p class="group">{section.group}</p>
            {/if}
            {#each section.rows as { item, index } (item.id)}
              <div
                id={optionId(item)}
                class="option"
                class:active={index === highlight}
                class:disabled={!item.enabled}
                role="option"
                tabindex="-1"
                aria-selected={index === highlight}
                aria-disabled={!item.enabled ? "true" : undefined}
                onclick={() => runItem(item)}
                onkeydown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    runItem(item);
                  }
                }}
              >
                <span>{item.title}</span>
                {#if item.chord}<kbd>{item.chord}</kbd>{/if}
              </div>
            {/each}
          {/each}
        {/if}
      </div>
      <p class="hint">Enter run · Esc close</p>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    align-items: start;
    justify-items: center;
    padding-top: 12vh;
    background: var(--overlay);
  }
  .palette {
    width: min(520px, calc(100vw - 32px));
    max-height: min(480px, 70vh);
    margin: 0;
    padding: 12px;
    overflow: hidden;
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 10px;
    background: var(--glass-strong);
    box-shadow: var(--shadow-float);
    color: var(--fg);
  }
  input {
    width: 100%;
    min-height: 36px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
    color: var(--fg);
  }
  .list {
    overflow-y: auto;
    display: grid;
    align-content: start;
    gap: 2px;
  }
  .group {
    margin: 8px 4px 4px;
    color: var(--fg-subtle);
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .group:first-child { margin-top: 0; }
  .option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .option.active { background: var(--select-option-selected); }
  .option.disabled {
    cursor: default;
    opacity: 0.55;
  }
  kbd {
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-sunken);
    color: var(--fg-muted);
    font: 650 11px/1.4 var(--font-mono, ui-monospace, monospace);
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--fg-muted);
    font-size: 11px;
  }
  .hint { padding: 0 4px; }
  .empty { padding: 12px 10px; }
</style>
