<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const win = getCurrentWindow();

  let maximized = $state(false);

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
    <span class="wordmark" data-tauri-drag-region>Origami</span>
  </div>

  <div class="controls">
    <button class="ctrl" onclick={minimize} aria-label="Minimize">
      <svg viewBox="0 0 12 12"><line x1="2" y1="6" x2="10" y2="6" /></svg>
    </button>
    <button
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
    <button class="ctrl close" onclick={close} aria-label="Close">
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
    background: var(--bg-raised);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    height: 100%;
  }

  .wordmark {
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

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
    background: var(--bg-sunken);
    color: var(--fg);
  }

  .ctrl.close:hover {
    background: var(--danger);
    color: #fff;
  }

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
