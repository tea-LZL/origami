<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { app, cycleLayout } from "./stores.svelte";
  import OrigamiBird from "./OrigamiBird.svelte";

  const win = getCurrentWindow();

  let maximized = $state(false);
  let statuses = $derived(Object.values(app.value.accountStatuses));
  let pendingOperations = $derived(
    statuses.reduce((total, status) => total + status.pendingOperations, 0),
  );
  let aggregateState = $derived(
    statuses.some((status) => status.state === "error")
      ? "Attention"
      : statuses.some((status) => status.state === "syncing")
        ? "Syncing"
        : statuses.length ? "Online" : "Local",
  );
  let layoutLabel = $derived(
    app.value.layout === "three-pane"
      ? "Three-pane"
      : app.value.layout === "two-pane"
        ? "Two-pane"
        : "Reading focus",
  );

  async function minimize() {
    await win.minimize();
  }

  async function toggleMaximize() {
    await win.toggleMaximize();
    maximized = await win.isMaximized();
  }

  async function close() {
    await win.close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="brand-mark" data-tauri-drag-region><OrigamiBird size={18} alt="" /></span>
    <span class="wordmark" data-tauri-drag-region>Origami</span>
    <span class:error={aggregateState === "Attention"} class:syncing={aggregateState === "Syncing"} class="aggregate" data-tauri-drag-region aria-live="polite" title="Mail synchronization status">
      <i></i>{aggregateState}
    </span>
    {#if pendingOperations > 0}
      <span class="pending" data-tauri-drag-region aria-label={`${pendingOperations} pending outbox operations`}>Outbox {pendingOperations}</span>
    {/if}
  </div>

  <div class="controls">
    <button
      type="button"
      class="ctrl layout"
      onclick={cycleLayout}
      aria-label={`Workspace layout: ${layoutLabel}`}
      title={`Workspace layout: ${layoutLabel}`}
    >
      <svg viewBox="0 0 12 12">
        <rect x="1" y="2" width="3" height="8" rx=".5" />
        <rect x="5" y="2" width="3" height="8" rx=".5" />
        <rect x="9" y="2" width="2" height="8" rx=".5" />
      </svg>
    </button>
    <button type="button" class="ctrl settings" onclick={() => app.value.preferencesOpen = true} aria-label="Preferences">
      <svg viewBox="0 0 12 12">
        <circle cx="6" cy="6" r="2" />
        <path d="M6 1.5v1M6 9.5v1M1.5 6h1M9.5 6h1M2.8 2.8l.7.7M8.5 8.5l.7.7M9.2 2.8l-.7.7M3.5 8.5l-.7.7" />
      </svg>
    </button>
    <button type="button" class="ctrl" onclick={minimize} aria-label="Minimize">
      <svg viewBox="0 0 12 12"><line x1="2" y1="6" x2="10" y2="6" /></svg>
    </button>
    <button
      type="button"
      class="ctrl"
      onclick={toggleMaximize}
      aria-label={maximized ? "Restore" : "Maximize"}
    >
      {#if maximized}
        <svg viewBox="0 0 12 12">
          <rect x="2.5" y="4" width="6" height="6" rx="1" />
          <path d="M4.5 4V2.5a1 1 0 0 1 1-1H9.5a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H8" />
        </svg>
      {:else}
        <svg viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" rx="1" /></svg>
      {/if}
    </button>
    <button type="button" class="ctrl close" onclick={close} aria-label="Close to tray">
      <svg viewBox="0 0 12 12">
        <line x1="2.5" y1="2.5" x2="9.5" y2="9.5" />
        <line x1="9.5" y1="2.5" x2="2.5" y2="9.5" />
      </svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    height: var(--titlebar-height);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 14px;
    background: linear-gradient(180deg, color-mix(in oklab, var(--bg-raised) 94%, var(--paper-highlight)), var(--bg-raised));
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 -1px 0 var(--paper-highlight);
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    height: 100%;
  }
  .brand-mark { width: 18px; height: 18px; display: grid; place-items: center; filter: drop-shadow(0 2px 2px color-mix(in oklab, var(--accent) 20%, transparent)); }
  .brand-mark :global(img) { display: block; }

  .wordmark {
    font: 600 15px/1 var(--font-display);
    letter-spacing: 0.015em;
    color: var(--fg);
    user-select: none;
  }
  .aggregate {
    display: inline-flex; align-items: center; gap: 5px; margin-left: 4px;
    color: var(--fg-subtle); font-size: 9px; font-weight: 700; letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .aggregate i { width: 5px; height: 5px; border-radius: 50%; background: var(--success); }
  .aggregate.error { color: var(--danger); }
  .aggregate.error i { background: var(--danger); }
  .aggregate.syncing i { background: var(--accent); animation: status-pulse 1.2s ease-in-out infinite; }
  .pending {
    padding: 2px 6px; border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 11%, transparent);
    color: var(--accent); font-size: 9px; font-weight: 750;
  }
  @keyframes status-pulse { 50% { opacity: 0.35; transform: scale(0.8); } }

  .controls {
    display: flex;
    height: 100%;
  }

  .ctrl {
    width: 44px;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--fg-muted);
    transition:
      background var(--transition-fast),
      color var(--transition-fast);
  }

  .ctrl:hover {
    background: color-mix(in oklab, var(--bg-sunken) 88%, var(--accent));
    color: var(--fg);
  }

  .ctrl.close:hover {
    background: var(--danger);
    color: var(--danger-fg);
  }

  .ctrl:focus-visible { position: relative; z-index: 1; outline-offset: -3px; }

  .ctrl svg {
    width: 12px;
    height: 12px;
    stroke: currentColor;
    stroke-width: 1.4;
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
