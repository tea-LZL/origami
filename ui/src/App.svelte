<script lang="ts">
  import { onMount, type Component } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    app,
    applyThemePreference,
    bootstrap,
    closeComposer,
    commitPaneWidth,
    openComposer,
    updatePaneWidth,
    undoLastMessageAction,
  } from "./lib/stores.svelte";
  import { focusNavigationTarget, type NavigationShortcut } from "./lib/navigation";
  import Titlebar from "./lib/Titlebar.svelte";
  import OrigamiArtwork from "./lib/OrigamiArtwork.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import ThreadList from "./lib/ThreadList.svelte";
  import MessageView from "./lib/MessageView.svelte";
  import AddAccount from "./lib/AddAccount.svelte";
  import Preferences from "./lib/Preferences.svelte";
  import Outbox from "./lib/Outbox.svelte";
  import PaneSplitter from "./lib/PaneSplitter.svelte";

  interface AppInfo {
    name: string;
    coreVersion: string;
    shellVersion: string;
  }

  let info: AppInfo | null = $state(null);
  let addAccount: { show: () => void } | null = $state(null);
  let ComposerComponent: Component | null = $state(null);

  $effect(() => {
    if (app.value.composerOpen && !ComposerComponent) {
      import("./lib/Composer.svelte").then((module) => {
        ComposerComponent = module.default;
      });
    }
  });

  $effect(() => {
    const root = document.documentElement;
    applyThemePreference(app.value.theme);
    root.dataset.density = app.value.density;
    root.dataset.motion = app.value.motion;
  });

  onMount(async () => {
    try {
      info = await invoke<AppInfo>("app_info");
    } catch (error) {
      info = { name: "Origami", coreVersion: "unknown", shellVersion: "unknown" };
      app.value.lastError = `Could not read application diagnostics: ${String(error)}`;
    }
    await bootstrap();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (app.value.preferencesOpen) {
        e.preventDefault();
        app.value.preferencesOpen = false;
        return;
      }
      if (app.value.outboxAccountId) {
        e.preventDefault();
        app.value.outboxAccountId = null;
        return;
      }
      if (app.value.composerOpen) {
        e.preventDefault();
        closeComposer();
      }
      return;
    }
    const target = e.target as HTMLElement | null;
    const protectedTarget = target?.closest(
      "input, textarea, select, [contenteditable='true'], [role='dialog'], [role='menu'], [popover], [aria-haspopup='listbox']",
    );
    if (
      e.altKey
      && !e.metaKey
      && !e.ctrlKey
      && !e.shiftKey
      && ["1", "2", "3"].includes(e.key)
    ) {
      if (protectedTarget || app.value.composerOpen || app.value.preferencesOpen || app.value.outboxAccountId) return;
      e.preventDefault();
      focusNavigationTarget(e.key as NavigationShortcut, app.value.selectedEnvelope?.id ?? null);
      return;
    }
    if (
      e.defaultPrevented
      || e.metaKey
      || e.ctrlKey
      || e.altKey
      || target?.closest("input, textarea, select, button, [contenteditable='true'], [role='dialog'], [role='menu']")
    ) return;
    if (e.key === "n" && !app.value.composerOpen) {
      e.preventDefault();
      openComposer();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <Titlebar />

  <main class="content" aria-label="Origami mail workspace" aria-keyshortcuts="Alt+1 Alt+2 Alt+3">
    {#if !info || !app.value.ready}
      <div class="loading" role="status" aria-live="polite">
        <OrigamiArtwork variant="launch" theme={app.value.theme} className="launch-art" loading="eager" />
        <p>Loading Origami…</p>
      </div>
    {:else if app.value.accounts.length === 0}
      <div class="empty">
        <OrigamiArtwork variant="hero" theme={app.value.theme} className="hero-art" />
        <h2>No accounts configured</h2>
        <p>Add an email account to get started.</p>
        <button class="add-btn" onclick={() => addAccount?.show()}>Add account</button>
      </div>
    {:else}
      <div
        class="pane-row"
        class:two-pane={app.value.layout === "two-pane"}
        class:reading={app.value.layout === "reading"}
        class:message-active={app.value.selectedEnvelope !== null}
        style={`--sidebar-width: ${app.value.sidebarWidth}px; --thread-width: ${app.value.threadListWidth}px;`}
      >
        {#if app.value.layout === "three-pane"}
          <Sidebar onadd={() => addAccount?.show()} />
          <PaneSplitter
            value={app.value.sidebarWidth}
            min={200}
            max={420}
            label="Sidebar width"
            onResize={(width) => updatePaneWidth("sidebar", width)}
            onResizeEnd={commitPaneWidth}
          />
        {/if}
        {#if app.value.layout !== "reading"}
          <ThreadList />
          <PaneSplitter
            value={app.value.threadListWidth}
            min={280}
            max={520}
            label="Message list width"
            onResize={(width) => updatePaneWidth("threadList", width)}
            onResizeEnd={commitPaneWidth}
          />
        {/if}
        <MessageView />
      </div>
    {/if}
  </main>

  {#if info && app.value.ready && app.value.accounts.length > 0}
    <button
      class="fab"
      type="button"
      onclick={() => openComposer()}
      aria-label="Compose new message"
      aria-keyshortcuts="N"
      title="Compose new message (N)"
    >Compose</button>
  {/if}
  {#if ComposerComponent}
    <ComposerComponent />
  {/if}
  <AddAccount bind:this={addAccount} />
  <Preferences />
  <Outbox />

  {#if app.value.lastError}
    <button class="toast" type="button" aria-live="assertive" onclick={() => (app.value.lastError = null)}>
      {app.value.lastError}
    </button>
  {/if}
  {#if app.value.undoAction}
    <div class="undo-toast" role="status">
      <span>{app.value.undoAction.message}</span>
      <button type="button" onclick={undoLastMessageAction}>Undo</button>
    </div>
  {/if}
  {#if app.value.lastNotice}
    <button class="notice-toast" type="button" onclick={() => app.value.lastNotice = null}>
      {app.value.lastNotice}
    </button>
  {/if}
</div>

<style>
   .shell { height: 100%; display: flex; flex-direction: column; background: transparent; }
  .content { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .pane-row { flex: 1; display: flex; min-height: 0; animation: surface-in var(--transition-med) both; }
   .loading, .empty {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 12px;
     color: var(--fg-muted); text-align: center; padding: 24px;
   }
   .loading { letter-spacing: 0.02em; }
   .empty { position: relative; }
   :global(.launch-art) { width: 88px; border-radius: 18px; box-shadow: var(--shadow-float); }
   :global(.hero-art) { width: min(220px, 52vw); margin-bottom: 6px; }
   .empty h2 { margin: 4px 0 0; color: var(--fg); font: 400 clamp(26px, 4vw, 36px)/1.06 var(--font-display); letter-spacing: -0.025em; }
   .empty p { max-width: 420px; margin: 0; line-height: 1.6; }
  .fab {
    position: fixed;
    right: 24px;
    bottom: 24px;
    min-width: 56px;
    height: 44px;
    padding: 0 16px;
     border-radius: var(--radius-sm);
    background: var(--accent);
    color: var(--accent-fg);
    font-size: 13px;
    font-weight: 750;
     box-shadow: var(--shadow-sheet);
    z-index: 10;
    animation: surface-in var(--transition-med) both;
  }
  .fab:hover { background: var(--accent-hover); transform: translateY(-2px); }
  .fab:active { transform: translateY(1px); }
  .add-btn {
    margin-top: 12px;
    background: var(--accent);
    color: var(--accent-fg);
    padding: 10px 24px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 14px;
    min-height: 36px;
  }
   .add-btn:hover { background: var(--accent-hover); }
  .toast {
    position: fixed; left: 50%; bottom: 24px;
    translate: -50% 0;
    background: var(--danger);
    color: var(--danger-fg);
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-size: 13px;
    cursor: pointer;
    z-index: 100;
  }
  .undo-toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    z-index: 101;
    translate: -50% 0;
    padding: 9px 10px 9px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    gap: 18px;
    background: var(--fg);
    color: var(--bg-raised);
    box-shadow: var(--shadow-panel);
    font-size: 12px;
  }
  .undo-toast button {
    padding: 4px 8px;
    border-radius: 4px;
    background: var(--accent);
    color: var(--accent-fg);
    font-weight: 750;
  }
  .notice-toast {
    position: fixed;
    right: 24px;
    bottom: 92px;
    z-index: 100;
    max-width: 360px;
    padding: 9px 13px;
    border: 1px solid color-mix(in oklab, var(--accent) 35%, var(--border));
    border-radius: var(--radius-md);
    background: var(--bg-raised);
    color: var(--fg);
    box-shadow: var(--shadow-panel);
    font-size: 12px;
    text-align: left;
  }

  @media (max-width: 760px) {
    .pane-row {
      position: relative;
    }
    .pane-row :global(.splitter) {
      display: none;
    }
    .pane-row :global(.sidebar) {
      display: none;
    }
    .pane-row :global(.threadlist) {
      width: 100%;
      min-width: 0;
      border-right: 0;
    }
    .pane-row :global(.message) {
      width: 100%;
      padding: 18px 16px;
    }
    .pane-row:not(.reading):not(.message-active) :global(.message) {
      display: none;
    }
    .pane-row.message-active:not(.reading) :global(.threadlist) {
      display: none;
    }
    .fab {
      right: 16px;
      bottom: 16px;
    }
    .toast {
      right: 16px;
      left: 16px;
      bottom: 16px;
      translate: none;
      text-align: left;
    }
    .undo-toast {
      right: 16px;
      left: 16px;
      bottom: 16px;
      translate: none;
      justify-content: space-between;
    }
    .notice-toast {
      right: 16px;
      bottom: 84px;
      max-width: calc(100vw - 32px);
    }
  }
</style>
