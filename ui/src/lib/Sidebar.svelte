<script lang="ts">
  import {
    app,
    selectFolder,
    selectUnifiedInbox,
    syncNow,
    removeAccount,
    reconnectAccount,
  } from "./stores.svelte";
  import { api } from "./api";
  import type { Mailbox } from "./types";
  import AccountSettings from "./AccountSettings.svelte";
  import { sidebarKeyboard } from "./navigation";
  import { sidebarFolders } from "./folderNav";

  interface Props {
    onadd?: () => void;
  }

  let { onadd }: Props = $props();

  let contextMenu: { x: number; y: number; accountId: string } | null = $state(null);
  let showRemoveConfirm = $state<string | null>(null);
  let showSettings = $state<string | null>(null);
  let expandedError = $state<string | null>(null);
  let folderMenu: { x: number; y: number; folder: Mailbox } | null = $state(null);
  let folderDialog: { mode: "create" | "rename"; accountId: string; folderId?: string } | null = $state(null);
  let folderName = $state("");
  let folderDelete = $state<Mailbox | null>(null);
  let folderBusy = $state(false);

  function foldersFor(accountDbId: string): Mailbox[] {
    return sidebarFolders(app.value.folders.filter((f) => f.accountId === accountDbId));
  }

  function onContextMenu(e: MouseEvent, accountId: string) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, accountId };
  }

  function closeContext() {
    contextMenu = null;
    folderMenu = null;
  }

  function roleLabel(role: string): string {
    const map: Record<string, string> = {
      Inbox: "Inbox", Sent: "Sent", Drafts: "Drafts", Trash: "Trash",
      Archive: "Archive", Junk: "Junk",
    };
    return map[role] ?? role;
  }

  function onSyncAccount(accountId: string) {
    closeContext();
    syncNow(accountId);
  }

  function onEditAccount(accountId: string) {
    closeContext();
    showSettings = accountId;
  }

  function onRemoveAccount(accountId: string) {
    closeContext();
    showRemoveConfirm = accountId;
  }

  async function confirmRemove() {
    if (!showRemoveConfirm) return;
    await removeAccount(showRemoveConfirm);
    showRemoveConfirm = null;
  }

  function onGlobalClick() {
    closeContext();
  }

  function openCreateFolder(accountId: string) {
    closeContext();
    folderName = "";
    folderDialog = { mode: "create", accountId };
  }

  function openFolderMenu(event: MouseEvent, folder: Mailbox) {
    if (folder.role !== "Other") return;
    event.preventDefault();
    event.stopPropagation();
    folderMenu = { x: event.clientX, y: event.clientY, folder };
  }

  async function submitFolder() {
    if (!folderDialog || !folderName.trim()) return;
    folderBusy = true;
    try {
      if (folderDialog.mode === "create") {
        await api.createFolder(folderDialog.accountId, folderName);
      } else {
        await api.renameFolder(folderDialog.folderId!, folderName);
      }
      app.value.folders = await api.listFolders();
      folderDialog = null;
    } catch (error) {
      app.value.lastError = String(error);
    } finally {
      folderBusy = false;
    }
  }

  async function confirmFolderDelete() {
    if (!folderDelete) return;
    folderBusy = true;
    try {
      await api.deleteFolder(folderDelete.id);
      app.value.folders = await api.listFolders();
      if (app.value.selectedFolderId === folderDelete.id) {
        app.value.selectedFolderId = null;
        app.value.envelopes = [];
        app.value.message = null;
      }
      folderDelete = null;
    } catch (error) {
      app.value.lastError = String(error);
    } finally {
      folderBusy = false;
    }
  }

  function statusTitle(message: string): string {
    const text = message.toLowerCase();
    if (/authentication failed|could not authenticate|invalid credentials|oauth|token expired|login failed|invalid login/.test(text)) return "Sign-in required";
    if (/connect|network|timeout|dns|unreachable/.test(text)) return "Account offline";
    return "Sync needs attention";
  }

  function statusMessage(message: string): string {
    const text = message.toLowerCase();
    if (/authentication failed|could not authenticate|invalid credentials|oauth|token expired|login failed|invalid login/.test(text)) {
      return "Origami could not authenticate. Reconnect this account to resume mail sync.";
    }
    if (/connect|network|timeout|dns|unreachable/.test(text)) {
      return "Origami cannot reach the mail server. Check your connection or retry.";
    }
    return "Some mail could not be synchronized. Your downloaded messages remain available.";
  }
</script>

<svelte:window onclick={onGlobalClick} />

<aside class="sidebar" data-navigation="sidebar" use:sidebarKeyboard>
  <div class="account-scroll">
    <button
      type="button"
      class="unified"
      class:selected={app.value.unifiedInbox}
      onclick={selectUnifiedInbox}
    >
      <span>Unified Inbox</span>
      <span>{app.value.folders.filter((folder) => folder.role === "Inbox").reduce((sum, folder) => sum + folder.unread, 0)}</span>
    </button>
    {#each app.value.accounts as account (account.id)}
      {@const folders = foldersFor(account.dbId)}
      {@const accountError = app.value.accountErrors[account.id]}
      {@const accountStatus = app.value.accountStatuses[account.id]}
      <div class="account-group">
        <div
          role="group"
          class="account-header"
          oncontextmenu={(e) => onContextMenu(e, account.id)}
        >
          <span class="name">{account.name}</span>
          {#if accountError}
            <button
              type="button"
              class="status-pill"
              aria-expanded={expandedError === account.id}
              onclick={(event) => {
                event.stopPropagation();
                expandedError = expandedError === account.id ? null : account.id;
              }}
            >Attention</button>
          {:else if accountStatus}
            <span class:syncing={accountStatus.state === "syncing"} class="runtime-status">
              <i></i>{accountStatus.state === "syncing" ? "Syncing" : "Online"}
            </span>
          {/if}
          <button
            type="button"
            class="gear"
            onclick={(e) => { e.stopPropagation(); onContextMenu(e, account.id); }}
            aria-label="Account menu"
          >•••</button>
        </div>
        {#if accountError && expandedError === account.id}
          <section class="status-card" aria-label="Account status">
            <strong>{statusTitle(accountError)}</strong>
            <p>{statusMessage(accountError)}</p>
            <div class="status-actions">
              <button type="button" onclick={() => reconnectAccount(account.id)} disabled={app.value.syncing}>
                {app.value.syncing ? "Retrying…" : "Retry"}
              </button>
              <button type="button" onclick={() => onEditAccount(account.id)}>Settings</button>
            </div>
            <details>
              <summary>Technical details</summary>
              <pre>{accountError}</pre>
            </details>
          </section>
        {/if}
        {#if accountStatus?.pendingOperations}
          <button type="button" class="outbox-status" onclick={() => app.value.outboxAccountId = account.id}>
            <span>Outbox</span>
            <strong>{accountStatus.pendingOperations}</strong>
            <small>pending {accountStatus.pendingOperations === 1 ? "operation" : "operations"}</small>
          </button>
        {/if}
        <nav aria-label={`${account.name} folders`}>
          {#each folders as folder (folder.id)}
            <button
              type="button"
              class="folder"
              class:selected={app.value.selectedFolderId === folder.id}
              aria-current={app.value.selectedFolderId === folder.id ? "page" : undefined}
              onclick={() => selectFolder(folder.id)}
              oncontextmenu={(event) => openFolderMenu(event, folder)}
            >
              <span class="folder-name">
                {roleLabel(folder.role) === "Other" ? folder.name : roleLabel(folder.role)}
              </span>
              <span class="counts">
                {#if folder.unread > 0}
                  <span class="unread">{folder.unread}</span>
                {/if}
                <span class="total">{folder.total}</span>
              </span>
            </button>
          {/each}
        </nav>
      </div>
    {/each}

    {#if app.value.accounts.length === 0}
      <div class="empty">No accounts configured.</div>
    {/if}
  </div>
  <button type="button" class="add-account" onclick={() => onadd?.()}>+ Add account</button>
</aside>

<!-- Context menu -->
{#if contextMenu}
  {@const id = contextMenu.accountId}
  <div
    class="context-menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    role="menu"
  >
    <button type="button" role="menuitem" onclick={() => onSyncAccount(id)}>Sync now</button>
    <button type="button" role="menuitem" onclick={() => openCreateFolder(id)}>Create folder…</button>
    <button type="button" role="menuitem" onclick={() => onEditAccount(id)}>Settings…</button>
    <button type="button" role="menuitem" class="danger" onclick={() => onRemoveAccount(id)}>Remove account</button>
  </div>
{/if}

{#if folderMenu}
  <div class="context-menu" style:left="{folderMenu.x}px" style:top="{folderMenu.y}px" role="menu">
    <button type="button" role="menuitem" onclick={() => {
      folderName = folderMenu!.folder.name;
      folderDialog = { mode: "rename", accountId: "", folderId: folderMenu!.folder.id };
      folderMenu = null;
    }}>Rename…</button>
    <button type="button" role="menuitem" class="danger" onclick={() => {
      folderDelete = folderMenu!.folder;
      folderMenu = null;
    }}>Delete folder</button>
  </div>
{/if}

{#if folderDialog}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="folder-dialog-title">
    <form class="dialog" onsubmit={(event) => { event.preventDefault(); submitFolder(); }}>
      <h3 id="folder-dialog-title">{folderDialog.mode === "create" ? "Create folder" : "Rename folder"}</h3>
      <label>Folder name <input bind:value={folderName} /></label>
      <div class="actions">
        <button type="button" onclick={() => folderDialog = null}>Cancel</button>
        <button type="submit" disabled={folderBusy}>{folderBusy ? "Saving…" : "Save"}</button>
      </div>
    </form>
  </div>
{/if}

{#if folderDelete}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="delete-folder-title">
    <div class="dialog">
      <h3 id="delete-folder-title">Delete {folderDelete.name}?</h3>
      <p>The server folder and all messages inside it will be permanently deleted.</p>
      <div class="actions">
        <button type="button" onclick={() => folderDelete = null}>Cancel</button>
        <button type="button" class="danger-btn" onclick={confirmFolderDelete} disabled={folderBusy}>Delete</button>
      </div>
    </div>
  </div>
{/if}

<!-- Remove confirmation -->
{#if showRemoveConfirm}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="remove-account-title">
    <div class="dialog">
      <h3 id="remove-account-title">Remove account?</h3>
      <p>This will stop sync and delete the saved credentials. The local database is not cleared.</p>
      <div class="actions">
        <button type="button" onclick={() => showRemoveConfirm = null}>Cancel</button>
        <button type="button" class="danger-btn" onclick={confirmRemove}>Remove</button>
      </div>
    </div>
  </div>
{/if}

<AccountSettings
  open={showSettings !== null}
  accountId={showSettings ?? ""}
  onclose={() => showSettings = null}
/>

<style>
  .sidebar {
    width: var(--sidebar-width, 260px);
    min-width: 200px;
    flex-shrink: 0;
    background: linear-gradient(100deg, color-mix(in oklab, var(--bg-raised) 92%, var(--accent)), var(--bg-raised));
    border-right: 1px solid var(--border);
    box-shadow: inset -1px 0 var(--paper-highlight);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .account-scroll { flex: 1; min-height: 0; overflow-y: auto; scrollbar-gutter: stable; }
  .unified {
    position: relative;
    width: calc(100% - 16px);
    margin: 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    display: flex;
    justify-content: space-between;
    color: var(--fg-muted);
    font-size: 12px;
    font-weight: 750;
    min-height: 36px;
    transition: background-color var(--transition-fast), color var(--transition-fast), transform var(--transition-fast);
  }
  .unified:hover { background: color-mix(in oklab, var(--bg-sunken) 86%, var(--accent)); color: var(--fg); transform: translateX(2px); }
  .unified.selected {
    background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised));
    color: var(--accent);
  }

  .account-group { border-bottom: 1px solid color-mix(in oklab, var(--paper-rule) 68%, transparent); }

  .account-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 6px;
    font-size: 13px;
    font-weight: 700;
    color: var(--fg);
    user-select: none;
  }

  .status-pill {
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--danger) 14%, transparent);
    color: var(--danger);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
  }
  .runtime-status {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--fg-subtle); font-size: 9px; font-weight: 700; text-transform: uppercase;
  }
  .runtime-status i { width: 5px; height: 5px; border-radius: 50%; background: var(--success); }
  .runtime-status.syncing i { background: var(--accent); animation: status-pulse 1.2s ease-in-out infinite; }
  .outbox-status {
    width: calc(100% - 20px); display: flex; align-items: baseline; gap: 6px; margin: 0 10px 5px; padding: 5px 8px;
    border-radius: var(--radius-sm); background: color-mix(in oklab, var(--accent) 8%, transparent);
    color: var(--fg-muted); font-size: 10px;
  }
  .outbox-status span { color: var(--accent); font-weight: 750; }
  .outbox-status strong { color: var(--fg); }
  .outbox-status small { margin-left: auto; color: var(--fg-subtle); }
  .outbox-status:hover { background: color-mix(in oklab, var(--accent) 14%, transparent); }
  @keyframes status-pulse { 50% { opacity: 0.35; transform: scale(0.8); } }

  .status-card {
    margin: 2px 10px 8px;
    padding: 10px;
    border: 1px solid color-mix(in oklab, var(--danger) 36%, var(--border));
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--danger) 7%, var(--bg-raised));
    font-size: 11px;
  }
  .status-card strong { color: var(--fg); font-size: 12px; }
  .status-card p { margin: 4px 0 9px; color: var(--fg-muted); line-height: 1.4; }
  .status-actions { display: flex; gap: 6px; }
  .status-actions button {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    font-weight: 600;
  }
  .status-actions button:first-child { background: var(--accent); color: var(--accent-fg); }
  .status-actions button:disabled { opacity: 0.55; cursor: wait; }
  .status-card details { margin-top: 8px; color: var(--fg-muted); }
  .status-card summary { cursor: pointer; }
  .status-card pre {
    margin: 6px 0 0;
    padding: 7px;
    overflow: auto;
    border-radius: 4px;
    background: var(--bg-sunken);
    font: 10px/1.4 var(--font-mono);
    user-select: text;
    white-space: pre-wrap;
  }

  .gear {
    margin-left: auto;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    font-size: 11px;
    opacity: 0;
    transition: opacity var(--transition-fast), background-color var(--transition-fast), transform var(--transition-fast);
  }
  .account-header:hover .gear { opacity: 0.7; }
  .gear:focus-visible { opacity: 1; }
  .gear:hover { opacity: 1 !important; background: var(--bg-sunken); }

  nav { padding: 2px 8px 8px; }
  .folder {
    position: relative;
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    color: var(--fg);
    font-size: 12px;
    min-height: 32px;
    transition: background-color var(--transition-fast), color var(--transition-fast), transform var(--transition-fast);
  }
  .folder:hover { background: color-mix(in oklab, var(--bg-sunken) 88%, var(--accent)); transform: translateX(2px); }
  .folder.selected {
    background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised));
    color: var(--accent);
  }
  .unified.selected::before,
  .folder.selected::before {
    content: "";
    position: absolute;
    top: 7px;
    bottom: 7px;
    left: 5px;
    width: 3px;
    border-radius: 999px;
    background: var(--accent);
    pointer-events: none;
  }
  .unified:focus-visible,
  .folder:focus-visible {
    outline-offset: -2px;
  }
  .folder-name { text-align: left; }
  .counts { display: flex; gap: 6px; align-items: center; }
  .unread {
    background: var(--accent);
    color: var(--accent-fg);
    border-radius: 10px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 600;
    min-width: 16px;
    text-align: center;
  }
  .total { color: var(--fg-subtle); font-size: 10px; }

  .empty {
    padding: 24px;
    color: var(--fg-muted);
    text-align: center;
    font-size: 12px;
  }

  .add-account {
    margin: 8px;
    padding: 7px 10px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    color: var(--fg-muted);
    font-size: 12px;
    font-weight: 600;
    min-height: 36px;
  }
  .add-account:hover { border-color: var(--accent); color: var(--accent); background: var(--bg-sunken); }

  /* Context menu */
  .context-menu {
    position: fixed;
    z-index: 60;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-float);
    padding: 4px;
    min-width: 180px;
    display: flex;
    flex-direction: column;
    animation: surface-in var(--transition-fast) both;
  }
  .context-menu button {
    padding: 6px 14px;
    font-size: 12px;
    text-align: left;
    border-radius: 4px;
    color: var(--fg);
    min-height: 32px;
  }
  .context-menu button:hover { background: var(--bg-sunken); }
  .context-menu .danger { color: var(--danger); }
  .context-menu .danger:hover { background: color-mix(in oklab, var(--danger) 12%, transparent); }

  /* Confirm dialog */
  .overlay {
    position: fixed; inset: 0;
    background: var(--overlay);
    display: grid; place-items: center; z-index: 70;
  }
  .dialog {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 24px;
    max-width: 380px;
    box-shadow: var(--shadow-float);
    animation: surface-in var(--transition-med) both;
  }
  .dialog h3 { margin: 0 0 8px; }
  .dialog p { color: var(--fg-muted); font-size: 13px; margin: 0 0 16px; }
  .dialog label { display: grid; gap: 5px; color: var(--fg-muted); font-size: 12px; }
  .dialog input {
    padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--radius-sm);
    background: var(--bg-sunken); color: var(--fg); font: inherit;
    min-height: 36px;
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .actions button {
    padding: 6px 16px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }
  .actions button:first-child { color: var(--fg-muted); }
  .actions button:first-child:hover { background: var(--bg-sunken); }
  .danger-btn { background: var(--danger); color: var(--danger-fg); font-weight: 600; }
  .danger-btn:hover { opacity: 0.8; }
</style>
