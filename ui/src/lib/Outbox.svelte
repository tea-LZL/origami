<script lang="ts">
  import { api, type OutboxSummary } from "./api";
  import { app, pollAccountErrors } from "./stores.svelte";

  let entries = $state<OutboxSummary[]>([]);
  let loading = $state(false);
  let retrying = $state(false);
  let error = $state("");
  let generation = 0;
  let accountName = $derived(
    app.value.accounts.find((account) => account.id === app.value.outboxAccountId)?.name ?? "Account",
  );

  $effect(() => {
    const accountId = app.value.outboxAccountId;
    if (!accountId) return;
    entries = [];
    void load(accountId);
  });

  async function load(accountId: string) {
    const request = ++generation;
    loading = true;
    error = "";
    try {
      const result = await api.listOutbox(accountId);
      if (request === generation && app.value.outboxAccountId === accountId) entries = result;
    } catch (cause) {
      if (request === generation) error = String(cause);
    } finally {
      if (request === generation) loading = false;
    }
  }

  async function retry() {
    const accountId = app.value.outboxAccountId;
    if (!accountId) return;
    retrying = true;
    error = "";
    try {
      await api.retryOutbox(accountId);
      await load(accountId);
      await pollAccountErrors();
    } catch (cause) {
      const message = String(cause);
      await load(accountId);
      error = message;
    } finally {
      retrying = false;
    }
  }

  function close() {
    generation += 1;
    app.value.outboxAccountId = null;
  }

  function queuedAt(timestamp: number): string {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(timestamp * 1000));
  }
</script>

{#if app.value.outboxAccountId}
  <div class="overlay">
    <dialog open aria-labelledby="outbox-title">
      <header>
        <div>
          <span>{accountName}</span>
          <h2 id="outbox-title">Outbox</h2>
        </div>
        <button class="close" onclick={close} aria-label="Close Outbox">×</button>
      </header>

      <p class="intro">Operations waiting for the mail server. Message contents and attachments are never shown here.</p>

      <div class="list" aria-live="polite">
        {#if loading && entries.length === 0}
          <p class="empty">Loading queued operations…</p>
        {:else if entries.length === 0}
          <div class="empty">
            <strong>All caught up</strong>
            <span>No operations are waiting to sync.</span>
          </div>
        {:else}
          {#each entries as entry (entry.id)}
            <article class:failed={entry.lastError !== null}>
              <div class="operation">
                <strong>{entry.kind}</strong>
                <time datetime={new Date(entry.createdAt * 1000).toISOString()}>{queuedAt(entry.createdAt)}</time>
              </div>
              <p>{entry.detail}</p>
              {#if entry.lastError}
                <details>
                  <summary>Failed {entry.attempts} {entry.attempts === 1 ? "time" : "times"}</summary>
                  <pre>{entry.lastError}</pre>
                </details>
              {:else}
                <span class="waiting">Waiting to sync</span>
              {/if}
            </article>
          {/each}
        {/if}
      </div>

      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <footer>
        <button onclick={close}>Close</button>
        <button class="primary" disabled={retrying || entries.length === 0} onclick={retry}>
          {retrying ? "Retrying…" : "Retry now"}
        </button>
      </footer>
    </dialog>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 85; display: grid; place-items: center;
    padding: 16px; background: rgba(5, 10, 24, 0.52);
  }
  dialog {
    position: static; width: min(560px, 100%); max-height: min(720px, calc(100vh - 32px));
    margin: 0; padding: 20px; overflow: hidden; border: 1px solid var(--border);
    border-radius: var(--radius-lg); display: grid; grid-template-rows: auto auto minmax(80px, 1fr) auto auto;
    gap: 12px; background: var(--bg-raised); box-shadow: 0 20px 60px rgba(0,0,0,0.34); color: var(--fg);
  }
  header { display: flex; align-items: flex-start; justify-content: space-between; }
  header span { color: var(--accent); font-size: 9px; font-weight: 800; letter-spacing: 0.12em; text-transform: uppercase; }
  h2 { margin: 1px 0 0; font-size: 20px; }
  .close { width: 28px; height: 28px; border-radius: var(--radius-sm); font-size: 20px; }
  .close:hover { background: var(--bg-sunken); }
  .intro { margin: 0; color: var(--fg-subtle); font-size: 11px; line-height: 1.45; }
  .list { overflow-y: auto; display: grid; align-content: start; gap: 8px; padding-right: 3px; }
  article { padding: 11px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-sunken); }
  article.failed { border-color: color-mix(in oklab, var(--danger) 34%, var(--border)); }
  .operation { display: flex; justify-content: space-between; gap: 12px; }
  .operation strong { font-size: 12px; }
  time, .waiting { color: var(--fg-subtle); font-size: 9px; }
  article p { margin: 4px 0 7px; color: var(--fg-muted); font-size: 11px; }
  details { color: var(--danger); font-size: 10px; }
  summary { cursor: pointer; font-weight: 700; }
  pre { margin: 6px 0 0; padding: 7px; overflow: auto; border-radius: var(--radius-sm); background: var(--bg-raised); color: var(--fg-muted); font: 10px/1.4 var(--font-mono); white-space: pre-wrap; user-select: text; }
  .empty { min-height: 100px; display: grid; place-content: center; gap: 4px; text-align: center; color: var(--fg-subtle); font-size: 11px; }
  .empty strong { color: var(--fg); font-size: 13px; }
  .error { margin: 0; color: var(--danger); font-size: 11px; }
  footer { display: flex; justify-content: flex-end; gap: 8px; }
  footer button { padding: 7px 12px; border-radius: var(--radius-sm); color: var(--fg-muted); }
  footer .primary { background: var(--accent); color: var(--accent-fg); font-weight: 700; }
  footer button:disabled { opacity: 0.55; }
</style>
