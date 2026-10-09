<script lang="ts">
  import {
    app,
    type ComposerSession,
    activateComposer,
    minimizeComposer,
    requestDiscardComposer,
  } from "./stores.svelte";

  function title(session: ComposerSession): string {
    const subject = session.draft.subject.trim();
    if (subject) return subject;
    const firstRecipient = session.draft.to
      .split(/[,\s]+/)
      .map((item) => item.trim())
      .find(Boolean);
    return firstRecipient ?? "New message";
  }

  function select(session: ComposerSession) {
    if (app.value.activeComposerId === session.id) {
      minimizeComposer();
    } else {
      activateComposer(session.id);
    }
  }

  function tabId(session: ComposerSession): string {
    return `compose-tab-${session.id}`;
  }

  function onTabKeydown(event: KeyboardEvent) {
    const keys = ["ArrowRight", "ArrowLeft", "Home", "End"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const list = (event.currentTarget as HTMLElement).closest('[role="tablist"]');
    const tabs = Array.from(list?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []);
    if (tabs.length === 0) return;
    const index = tabs.indexOf(event.currentTarget as HTMLButtonElement);
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : event.key === "ArrowRight"
          ? (index + 1) % tabs.length
          : (index - 1 + tabs.length) % tabs.length;
    tabs[next]?.focus();
  }

  $effect(() => {
    const id = app.value.activeComposerId;
    if (!id) return;
    const frame = requestAnimationFrame(() => {
      document
        .getElementById(`compose-tab-${id}`)
        ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

{#if app.value.composerSessions.length > 0}
  <div class="compose-tabs" role="tablist" aria-label="Open messages">
    {#each app.value.composerSessions as session (session.id)}
      {@const active = session.id === app.value.activeComposerId}
      <div class="tab" class:active>
        <button
          type="button"
          role="tab"
          id={tabId(session)}
          class="tab-button"
          aria-selected={active}
          aria-controls={`compose-pane-${session.id}`}
          tabindex={active ? 0 : -1}
          title={title(session)}
          onclick={() => select(session)}
          onkeydown={onTabKeydown}
        >
          {title(session)}
        </button>
        {#if session.saveState === "saving"}
          <span class="dot saving" role="status" aria-label="Saving draft"></span>
        {:else if session.syncState === "error"}
          <span class="dot error" role="status" aria-label="Sync failed"></span>
        {/if}
        <button
          type="button"
          class="tab-close"
          aria-label={`Discard draft: ${title(session)}`}
          onclick={() => requestDiscardComposer(session.id)}
        >×</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .compose-tabs {
    flex-shrink: 0;
    display: flex;
    align-items: stretch;
    gap: 2px;
    padding: 4px 8px 0;
    border-top: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-sunken) 70%, var(--bg-raised));
    overflow-x: auto;
  }
  .tab {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    max-width: 220px;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    color: var(--fg-muted);
  }
  .tab:hover { background: color-mix(in oklab, var(--accent) 7%, var(--bg-raised)); }
  .tab.active {
    background: var(--bg-raised);
    border-color: var(--border);
    box-shadow: inset 0 2px 0 var(--accent);
    color: var(--fg);
  }
  .tab-button {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 5px 4px 5px 10px;
    font-size: 11px;
    font-weight: 600;
  }
  .tab-close {
    width: 20px;
    height: 20px;
    margin-right: 4px;
    border-radius: 50%;
    font-size: 12px;
    color: var(--fg-subtle);
  }
  .tab-close:hover { background: var(--bg-sunken); color: var(--fg); }
  .dot {
    width: 7px;
    height: 7px;
    margin-inline: 3px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .dot.saving {
    background: var(--accent);
    animation: dot-pulse 1.2s ease-in-out infinite;
  }
  .dot.error { background: var(--warning); }
  @keyframes dot-pulse {
    0%, 100% { opacity: 0.35; }
    50% { opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .dot.saving { animation: none; }
  }
</style>
