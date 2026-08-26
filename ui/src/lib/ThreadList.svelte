<script lang="ts">
  import {
    app,
    clearMessageSelection,
    deleteSelectedPermanently,
    loadMoreEnvelopes,
    moveSelectedToFolder,
    moveSelectedToRole,
    selectAllMessages,
    selectEnvelope,
    selectEnvelopeExclusive,
    searchMessages,
    saveCurrentSearch,
    removeSavedSearch,
    setLayout,
    setSelectedFlag,
    setSelectedKeyword,
    toggleEnvelopeSelection,
  } from "./stores.svelte";
  import VirtualList from "./VirtualList.svelte";
  import type { Envelope } from "./types";
  import { threadKey } from "./threads";
  import { highlightSearchText } from "./searchHighlight";
  import Select from "./Select.svelte";
  import MessageSelectCheckbox from "./MessageSelectCheckbox.svelte";
  import OrigamiArtwork from "./OrigamiArtwork.svelte";

  const rowHeight = $derived(app.value.density === "compact" ? 52 : 60);
  let listEl: HTMLElement | null = $state(null);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let labelsOpen = $state(false);
  let newLabel = $state("");
  let saveSearchOpen = $state(false);
  let savedSearchName = $state("");
  let expandedThreads: string[] = $state([]);
  let moveDestination = $state<string | null>(null);

  const selectedFolder = $derived(
    app.value.folders.find((folder) => folder.id === app.value.selectedFolderId) ?? null,
  );
  const selectedIds = $derived.by(() => new Set(app.value.selectedMessageIds));
  const selectedKeywords = $derived.by(() => {
    return Array.from(new Set(
      app.value.envelopes
        .filter((envelope) => selectedIds.has(envelope.id))
        .flatMap((envelope) => envelope.keywords),
    )).sort();
  });
  const threadCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const envelope of app.value.envelopes) {
      const key = threadKey(envelope);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  });
  const visibleEnvelopes = $derived.by(() => {
    const seen = new Set<string>();
    const expanded = new Set(expandedThreads);
    return app.value.envelopes.filter((envelope) => {
      const key = threadKey(envelope);
      if (expanded.has(key)) return true;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  });

  function addLabel() {
    if (!newLabel.trim()) return;
    setSelectedKeyword(newLabel, true);
    newLabel = "";
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || app.value.composerOpen) return;
    const target = event.target as Element | null;
    if (target?.closest("input, button, select, textarea, [contenteditable='true']")) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
      event.preventDefault();
      void selectAllMessages();
      return;
    }
    if (event.key === "Escape" && app.value.selectedMessageIds.length > 0) {
      event.preventDefault();
      clearMessageSelection();
      return;
    }
    if (event.key === " " && app.value.selectedEnvelope) {
      event.preventDefault();
      toggleEnvelopeSelection(app.value.selectedEnvelope, event.shiftKey);
      return;
    }
    if (!["j", "k", "ArrowDown", "ArrowUp"].includes(event.key)) return;

    const envelopes = visibleEnvelopes;
    if (envelopes.length === 0) return;
    event.preventDefault();
    const current = envelopes.findIndex((item) => item.id === app.value.selectedEnvelope?.id);
    const nextIndex = event.key === "j" || event.key === "ArrowDown"
      ? Math.min(current + 1, envelopes.length - 1)
      : Math.max(current - 1, 0);
    const next = envelopes[nextIndex];
    if (event.shiftKey) {
      toggleEnvelopeSelection(next, true);
      selectEnvelope(next);
    } else {
      selectEnvelopeExclusive(next);
    }
    requestAnimationFrame(() => {
      listEl
        ?.querySelector<HTMLButtonElement>(`[data-item-id="${CSS.escape(next.id)}"]`)
        ?.focus();
    });
  }

  function messageListKeyboard(node: HTMLElement) {
    listEl = node;
    node.addEventListener("keydown", onKeydown);
    return {
      destroy() {
        node.removeEventListener("keydown", onKeydown);
        if (listEl === node) listEl = null;
      },
    };
  }

  function fromName(e: Envelope): string {
    return e.from[0]?.name ?? e.from[0]?.addr ?? "(unknown)";
  }

  function snippet(e: Envelope): string {
    return e.subject.length === 0 ? "(no subject)" : e.subject;
  }

  function isUnread(e: Envelope): boolean {
    return !e.flags.includes("Seen");
  }

  function onRowSelect(envelope: Envelope, event: MouseEvent | KeyboardEvent) {
    if (event.shiftKey) {
      toggleEnvelopeSelection(envelope, true);
      selectEnvelope(envelope);
    } else if (event.metaKey || event.ctrlKey) {
      toggleEnvelopeSelection(envelope);
      selectEnvelope(envelope);
    } else {
      selectEnvelopeExclusive(envelope);
    }
  }

  function toggleThread(event: MouseEvent, envelope: Envelope) {
    event.stopPropagation();
    const key = threadKey(envelope);
    expandedThreads = expandedThreads.includes(key)
      ? expandedThreads.filter((item) => item !== key)
      : [...expandedThreads, key];
  }

  function onSearch(event: Event) {
    const query = (event.currentTarget as HTMLInputElement).value;
    app.value.searchQuery = query;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => searchMessages(query), 180);
  }

  async function saveSearch() {
    if (!savedSearchName.trim()) return;
    await saveCurrentSearch(savedSearchName);
    savedSearchName = "";
    saveSearchOpen = false;
  }

  // Keep tag colors theme-owned so light and dark modes retain readable contrast.
  const TAG_PALETTE = [
    "var(--tag-blue)", "var(--tag-violet)", "var(--tag-teal)", "var(--tag-green)",
    "var(--tag-amber)", "var(--tag-rose)",
  ];
  function tagColor(name: string): string {
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = ((hash << 5) - hash + name.charCodeAt(i)) | 0;
    }
    return TAG_PALETTE[Math.abs(hash) % TAG_PALETTE.length];
  }
</script>

<section class="threadlist" data-navigation="thread-pane" use:messageListKeyboard role="group" aria-label="Messages">
  <header class="list-header">
    <div>
      <span class="eyebrow">Mailbox</span>
      <h2>{app.value.unifiedInbox ? "Unified Inbox" : selectedFolder?.name ?? "Messages"}</h2>
    </div>
    <div class="header-actions">
      {#if app.value.layout === "two-pane"}
        <button type="button" class="layout-button" onclick={() => setLayout("three-pane")} title="Show folders">
          Folders
        </button>
      {/if}
      <button
        type="button"
        class="layout-button"
        onclick={() => setLayout("reading")}
        disabled={!app.value.selectedEnvelope}
        title="Focus reading"
      >
        Focus
      </button>
      <span class="message-count">{selectedFolder?.total ?? app.value.envelopes.length}</span>
    </div>
  </header>
  <div class="search-box">
    <span aria-hidden="true">⌕</span>
    <input
      type="search"
      value={app.value.searchQuery}
      oninput={onSearch}
      placeholder="Search all mail"
      aria-label="Search all mail"
    />
    {#if app.value.searchQuery.trim()}
      <button type="button" onclick={() => saveSearchOpen = !saveSearchOpen} aria-label="Save search">+</button>
    {/if}
  </div>
  {#if saveSearchOpen}
    <form class="save-search" onsubmit={(event) => { event.preventDefault(); saveSearch(); }}>
      <input bind:value={savedSearchName} placeholder="Search name" aria-label="Search name" />
      <button type="submit">Save</button>
    </form>
  {/if}
  {#if app.value.savedSearches.length > 0}
    <div class="saved-searches" aria-label="Saved searches">
      {#each app.value.savedSearches as saved (saved.id)}
        <span>
          <button type="button" onclick={() => searchMessages(saved.query)} title={saved.query}>{saved.name}</button>
          <button type="button" onclick={() => removeSavedSearch(saved.id)} aria-label={`Delete ${saved.name}`}>×</button>
        </span>
      {/each}
    </div>
  {/if}
  {#if app.value.selectedMessageIds.length > 0}
    <div class="bulk-bar" role="toolbar" aria-label="Selected messages">
      <strong>{app.value.selectedMessageIds.length} selected</strong>
      <button type="button" onclick={() => setSelectedFlag("Seen", true)}>Read</button>
      <button type="button" onclick={() => setSelectedFlag("Seen", false)}>Unread</button>
      <button type="button" onclick={() => setSelectedFlag("Flagged", true)}>Star</button>
      <button type="button" onclick={() => setSelectedFlag("Flagged", false)}>Unstar</button>
      <button type="button" onclick={() => moveSelectedToRole("Archive")}>Archive</button>
      <button type="button" onclick={() => moveSelectedToRole("Trash")}>Trash</button>
      <button type="button" onclick={() => moveSelectedToRole("Junk")}>Junk</button>
      <button type="button" aria-expanded={labelsOpen} onclick={() => labelsOpen = !labelsOpen}>Labels</button>
      {#if selectedFolder?.role === "Trash" && !app.value.searchQuery.trim()}
        <button type="button" class="danger" onclick={deleteSelectedPermanently}>Delete</button>
      {/if}
      {#if selectedFolder}
        <Select
          bind:value={moveDestination}
          ariaLabel="Move selected messages"
          placeholder="Move…"
          compact
          options={app.value.folders
            .filter((folder) => folder.accountId === selectedFolder.accountId && folder.id !== selectedFolder.id)
            .map((folder) => ({ value: folder.id, label: folder.name }))}
          onValueChange={(folderId) => {
            moveSelectedToFolder(folderId);
            moveDestination = null;
          }}
        />
      {/if}
       <button type="button" onclick={() => void selectAllMessages()} disabled={app.value.selectingAll}>
         {app.value.selectingAll ? "Selecting…" : "Select all"}
       </button>
      <button type="button" class="clear" onclick={clearMessageSelection} aria-label="Clear selection">×</button>
    </div>
    {#if labelsOpen}
      <div class="label-editor">
        <form onsubmit={(event) => { event.preventDefault(); addLabel(); }}>
          <input bind:value={newLabel} placeholder="New label" aria-label="New label" />
          <button type="submit">Add</button>
        </form>
        {#if selectedKeywords.length > 0}
          <div class="active-labels">
            {#each selectedKeywords as keyword}
              <button type="button" onclick={() => setSelectedKeyword(keyword, false)} title={`Remove ${keyword}`}>
                {keyword} ×
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
  <div class="list-body" aria-busy={app.value.envelopesLoading}>
    {#if app.value.envelopesLoading}
      <div class="message-loading" role="status" aria-live="polite">
        <span class="sr-only">{app.value.searching ? "Searching messages" : "Loading messages"}</span>
        <div class="skeleton-list" aria-hidden="true">
          {#each Array(7) as _}
            <div class="skeleton-row">
              <span class="skeleton skeleton-check"></span>
              <span class="skeleton skeleton-sender"></span>
              <span class="skeleton skeleton-date"></span>
              <span class="skeleton skeleton-subject"></span>
            </div>
          {/each}
        </div>
      </div>
    {:else if app.value.envelopes.length === 0}
      <div class="empty">
        <OrigamiArtwork variant="empty" theme={app.value.theme} className="empty-art" />
        <span>No messages here</span>
      </div>
    {:else}
      <VirtualList
        items={visibleEnvelopes}
        itemHeight={rowHeight}
        selectedId={(e) => e.id}
        activeId={app.value.selectedEnvelope?.id ?? null}
        isSelected={(e) => selectedIds.has(e.id)}
        onSelect={onRowSelect}
        onEndReached={loadMoreEnvelopes}
      >
        {#snippet render(item: Envelope)}
          {@const count = threadCounts.get(threadKey(item)) ?? 1}
          {@const sender = fromName(item)}
          {@const subject = snippet(item)}
          <div class="row-content" class:unread={isUnread(item)}>
            <MessageSelectCheckbox
              checked={selectedIds.has(item.id)}
              label={`Select message from ${sender}: ${subject}`}
              onToggle={(range) => toggleEnvelopeSelection(item, range)}
            />
            <div class="top">
              <span class="from">
                {#each highlightSearchText(sender, app.value.searchQuery) as part}
                  <span class:search-match={part.match}>{part.text}</span>
                {/each}
              </span>
              <span class="date">{item.date ?? ""}</span>
            </div>
            <div class="subject" aria-label={subject}>
              {#each highlightSearchText(subject, app.value.searchQuery) as part}
                <span class:search-match={part.match}>{part.text}</span>
              {/each}
              {#if count > 1}
                <button
                  type="button"
                  class="thread-count"
                  onclick={(event) => toggleThread(event, item)}
                  aria-label={`${expandedThreads.includes(threadKey(item)) ? "Collapse" : "Expand"} ${count} message conversation`}
                >{expandedThreads.includes(threadKey(item)) ? "−" : "+"}{count}</button>
              {/if}
              {#each item.keywords as tag}
                <span class="tag" style:background={tagColor(tag)}>{tag}</span>
              {/each}
              {#if item.hasAttachment}<span class="attach" title="has attachment">@</span>{/if}
            </div>
          </div>
        {/snippet}
      </VirtualList>
      {#if app.value.envelopesLoadingMore}
        <div class="loading-more">Loading more…</div>
      {/if}
    {/if}
  </div>
</section>

<style>
  .threadlist {
    width: var(--thread-width, 360px);
    min-width: 280px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg) 86%, var(--bg-raised));
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .list-header {
    min-height: 64px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: linear-gradient(180deg, var(--bg-raised), color-mix(in oklab, var(--bg-raised) 92%, var(--bg-sunken)));
    box-shadow: inset 0 1px var(--paper-highlight);
  }
  .header-actions { display: flex; align-items: center; gap: 6px; }
  .layout-button {
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--fg-muted);
    font-size: 10px;
    font-weight: 700;
  }
  .layout-button:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
  .layout-button:disabled { opacity: 0.45; cursor: not-allowed; }
  .eyebrow {
    display: block;
    color: var(--fg-subtle);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .list-header h2 { margin: 1px 0 0; font: 400 19px/1.1 var(--font-display); letter-spacing: -0.015em; }
  .message-count {
    min-width: 28px;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-sunken);
    color: var(--fg-muted);
    font-size: 11px;
    text-align: center;
  }
  .search-box {
    margin: 8px 10px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    gap: 6px;
    background: color-mix(in oklab, var(--bg-raised) 94%, var(--bg-sunken));
    color: var(--fg-subtle);
  }
  .search-box:focus-within { border-color: var(--accent); box-shadow: inset 0 1px var(--paper-highlight), 0 0 0 2px color-mix(in oklab, var(--accent) 12%, transparent); }
  .search-box input {
    min-width: 0;
    width: 100%;
    padding: 7px 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: 12px;
  }
  @media (forced-colors: active) {
    .search-box:focus-within {
      outline: 2px solid Highlight;
      outline-offset: 2px;
      box-shadow: none;
    }
  }
  .search-box button { color: var(--accent); font-size: 18px; }
  .save-search { margin: 0 10px 7px; display: flex; gap: 5px; }
  .save-search input {
    min-width: 0; flex: 1; padding: 5px 7px; border: 1px solid var(--border);
    border-radius: 4px; background: var(--bg-raised); color: var(--fg); font: inherit;
  }
  .save-search button { padding: 4px 8px; border-radius: 4px; background: var(--accent); color: var(--accent-fg); }
  .saved-searches { padding: 0 10px 7px; display: flex; gap: 5px; overflow-x: auto; }
  .saved-searches span {
    flex-shrink: 0; border: 1px solid var(--border); border-radius: 999px;
    display: flex; align-items: center; background: var(--bg-raised); font-size: 10px;
  }
  .saved-searches button { padding: 3px 6px; color: var(--fg-muted); }
  .saved-searches button:last-child { padding-left: 1px; }
  .bulk-bar {
    min-height: 40px;
    padding: 6px 9px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    align-content: center;
    flex-wrap: wrap;
    gap: 4px;
    background: color-mix(in oklab, var(--accent) 6%, var(--bg-raised));
  }
  .bulk-bar strong { flex: 1 0 100%; font-size: 11px; white-space: nowrap; }
  .bulk-bar button {
    min-height: 28px;
    padding: 4px 7px;
    border-radius: 4px;
    color: var(--fg-muted);
    font-size: 10px;
    font-weight: 650;
  }
  .bulk-bar button:hover { background: var(--bg-raised); color: var(--fg); }
  .bulk-bar .danger { color: var(--danger); }
  .bulk-bar .clear { font-size: 16px; line-height: 1; }
  .label-editor {
    padding: 7px 9px;
    border-bottom: 1px solid var(--border);
    display: grid;
    gap: 6px;
    background: var(--bg-raised);
  }
  .label-editor form { display: flex; gap: 5px; }
  .label-editor input {
    min-width: 0;
    flex: 1;
    padding: 5px 7px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-sunken);
    color: var(--fg);
    font: inherit;
    font-size: 11px;
  }
  .label-editor form button,
  .active-labels button {
    padding: 4px 7px;
    border-radius: 4px;
    background: var(--bg-sunken);
    color: var(--fg-muted);
    font-size: 10px;
  }
  .active-labels { display: flex; flex-wrap: wrap; gap: 4px; }
  .list-body { position: relative; flex: 1 1 0; min-height: 0; display: flex; overflow: hidden; }
  .message-loading { width: 100%; overflow: hidden; }
  .skeleton-list { width: 100%; }
  .skeleton-row {
    height: 60px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 52px;
    grid-template-rows: 14px 12px;
    gap: 7px 9px;
    align-content: center;
  }
  .skeleton-check { grid-row: 1 / 3; align-self: center; width: 16px; height: 16px; border-radius: 4px; }
  .skeleton-sender { width: min(72%, 180px); height: 10px; border-radius: 3px; }
  .skeleton-date { justify-self: end; width: 42px; height: 8px; border-radius: 3px; }
  .skeleton-subject { width: min(88%, 240px); height: 8px; border-radius: 3px; }
  :global(:root[data-density="compact"]) .skeleton-row { height: 52px; padding-block: 7px; }
  .loading-more {
    position: absolute;
    left: 50%;
    bottom: 10px;
    transform: translateX(-50%);
    padding: 4px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-raised);
    color: var(--fg-muted);
    box-shadow: 0 2px 8px rgba(0,0,0,0.12);
    font-size: 10px;
  }
  .empty {
    min-height: 100%;
    padding: 24px;
    display: grid;
    place-content: center;
    gap: 10px;
    color: var(--fg-muted);
    text-align: center;
    font-size: 13px;
  }
  :global(.empty-art) { width: 82px; margin-inline: auto; opacity: 0.72; }
  .row-content { position: relative; display: flex; flex-direction: column; gap: 2px; padding-left: 34px; }
  .top { display: flex; justify-content: space-between; gap: 8px; line-height: 1.25; }
  .from { font-size: 13px; color: var(--fg); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .search-match {
    padding: 0 2px;
    border-radius: 3px;
    background: color-mix(in oklab, var(--accent) 20%, transparent);
    color: var(--fg);
  }
  .date { font-size: 11px; color: var(--fg-subtle); white-space: nowrap; }
  .subject { font-size: 12px; line-height: 1.35; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .unread .from, .unread .subject { font-weight: 700; color: var(--fg); }
  .attach { margin-left: 4px; color: var(--accent); }
  .thread-count {
    margin-left: 5px;
    padding: 0 5px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 12%, transparent);
    color: var(--accent);
    font-size: 9px;
    font-weight: 750;
  }
  .tag {
    display: inline-block;
    padding: 0 5px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
    color: var(--tag-fg);
    margin-left: 4px;
    vertical-align: middle;
    line-height: 1.6;
  }
</style>
