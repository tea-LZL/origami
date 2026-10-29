<script lang="ts">
  import { app } from "./stores.svelte";
  import { catalog, formatChord, type CommandGroup } from "./commands";
  import { trapFocus } from "./trapFocus";

  const groups: { title: string; group: CommandGroup }[] = [
    { title: "Triage", group: "Triage" },
    { title: "Compose", group: "Compose" },
    { title: "Global", group: "Folders" },
    { title: "Help", group: "Help" },
  ];

  const navigation = [
    { action: "Move through the message list", keys: "j / k, ↓ / ↑" },
    { action: "Open the focused message", keys: "Enter" },
    { action: "Select or deselect the current message", keys: "Space" },
    { action: "Expand or collapse a conversation", keys: "t" },
    { action: "Focus the sidebar, list, or message", keys: "Alt+1 / 2 / 3" },
    { action: "Close a dialog, leave the composer, clear selection", keys: "Esc" },
  ];

  const shortcuts = $derived(catalog.filter((command) => command.keys.length > 0));

  function close() {
    app.value.shortcutsOpen = false;
  }
</script>

{#if app.value.shortcutsOpen}
  <div
    class="overlay"
    role="presentation"
    use:trapFocus={{ onEscape: close }}
    onclick={(event) => {
      if (event.target === event.currentTarget) close();
    }}
  >
    <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="shortcuts-title">
      <div class="sheet-header">
        <div>
          <span class="eyebrow">Origami</span>
          <h2 id="shortcuts-title">Keyboard shortcuts</h2>
        </div>
        <button type="button" onclick={close} aria-label="Close keyboard shortcuts">×</button>
      </div>

      <section aria-labelledby="shortcuts-navigation">
        <h3 id="shortcuts-navigation">Navigation</h3>
        <dl>
          {#each navigation as row}
            <dt>{row.action}</dt>
            <dd><kbd>{row.keys}</kbd></dd>
          {/each}
        </dl>
      </section>

      {#each groups as group}
        {@const rows = shortcuts.filter((command) => command.group === group.group)}
        {#if rows.length > 0}
          <section aria-labelledby={`shortcuts-${group.group}`}>
            <h3 id={`shortcuts-${group.group}`}>{group.title}</h3>
            <dl>
              {#each rows as command (command.id)}
                <dt>{command.title}</dt>
                <dd>
                  {#each command.keys as chord, index (index)}
                    {#if index > 0}<span class="sep">or</span>{/if}
                    <kbd>{formatChord(chord)}</kbd>
                  {/each}
                </dd>
              {/each}
            </dl>
          </section>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 32px;
    background: var(--overlay);
  }
  .sheet {
    width: min(720px, 100%);
    max-height: min(82vh, 760px);
    overflow-y: auto;
    padding: 22px 26px 26px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    display: grid;
    gap: 18px;
    background: linear-gradient(145deg, var(--bg-raised), color-mix(in oklab, var(--bg-raised) 94%, var(--bg-sunken)));
    box-shadow: var(--shadow-float);
    color: var(--fg);
  }
  .sheet-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
  }
  .eyebrow {
    color: var(--accent);
    font-size: var(--fs-9);
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  h2 { margin: 1px 0 0; font: 400 27px/1 var(--font-display); letter-spacing: -0.02em; }
  h3 {
    margin: 0;
    color: var(--fg-subtle);
    font-size: var(--fs-10);
    font-weight: 800;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .sheet-header button {
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-20);
  }
  .sheet-header button:hover { background: var(--bg-sunken); }
  section { display: grid; gap: 8px; }
  dl { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 7px 18px; margin: 0; }
  dt { color: var(--fg-muted); }
  dd { margin: 0; display: flex; align-items: center; gap: 6px; justify-self: end; }
  kbd {
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    box-shadow: inset 0 1px 0 var(--paper-highlight);
    color: var(--fg);
    font: 600 var(--fs-11)/1.4 var(--font-mono);
    white-space: nowrap;
  }
  .sep { color: var(--fg-subtle); font-size: var(--fs-10); }
</style>
