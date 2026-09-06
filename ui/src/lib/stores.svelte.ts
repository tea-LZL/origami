// Svelte 5 rune-based state. One file, one $state root object holding
// the whole app. Mutations go through small helpers to keep the API
// surface tidy.

import {
  api,
  onSyncEvent,
  type AccountDto,
  type MessageDto,
  type SavedSearch,
  type Correspondent,
  type AccountStatusDto,
} from "./api";
import type { Envelope, EnvelopeSource, Flag, Mailbox, MailboxRole } from "./types";

export type WorkspaceLayout = "three-pane" | "two-pane" | "reading";

export interface State {
  ready: boolean;
  accounts: AccountDto[];
  folders: Mailbox[];
  selectedFolderId: string | null;
  unifiedInbox: boolean;
  envelopes: Envelope[];
  envelopesLoading: boolean;
  envelopesLoadingMore: boolean;
  selectingAll: boolean;
  envelopePage: number;
  hasMoreEnvelopes: boolean;
  searchQuery: string;
  searching: boolean;
  savedSearches: SavedSearch[];
  correspondents: Correspondent[];
  selectedEnvelope: Envelope | null;
  selectedMessageIds: string[];
  selectionAnchorId: string | null;
  message: MessageDto | null;
  messageLoading: boolean;
  composerOpen: boolean;
  composerAccountId: string | null;
  sending: boolean;
  composerAttachments: {
    name: string;
    mime: string;
    size: number;
    dataBase64: string;
  }[];
  composerDraft: {
    to: string;
    cc: string;
    bcc: string;
    subject: string;
    html: string;
  };
  composerThreading: {
    inReplyTo: string | null;
    references: string[];
  };
  syncing: boolean;
  lastError: string | null;
  lastNotice: string | null;
  accountErrors: Record<string, string>;
  accountStatuses: Record<string, AccountStatusDto>;
  preferencesOpen: boolean;
  outboxAccountId: string | null;
  theme: "system" | "light" | "dark";
  density: "comfortable" | "compact";
  motion: "system" | "full" | "reduced";
  layout: WorkspaceLayout;
  sidebarWidth: number;
  threadListWidth: number;
  undoAction: { message: string; token: number } | null;
}

const initial: State = {
  ready: false,
  accounts: [],
  folders: [],
  selectedFolderId: null,
  unifiedInbox: false,
  envelopes: [],
  envelopesLoading: false,
  envelopesLoadingMore: false,
  selectingAll: false,
  envelopePage: 1,
  hasMoreEnvelopes: true,
  searchQuery: "",
  searching: false,
  savedSearches: [],
  correspondents: [],
  selectedEnvelope: null,
  selectedMessageIds: [],
  selectionAnchorId: null,
  message: null,
  messageLoading: false,
  composerOpen: false,
  composerAccountId: null,
  sending: false,
  composerAttachments: [],
  composerDraft: { to: "", cc: "", bcc: "", subject: "", html: "" },
  composerThreading: { inReplyTo: null, references: [] },
  syncing: false,
  lastError: null,
  lastNotice: null,
  accountErrors: {},
  accountStatuses: {},
  preferencesOpen: false,
  outboxAccountId: null,
  theme: "system",
  density: "comfortable",
  motion: "system",
  layout: "three-pane",
  sidebarWidth: 260,
  threadListWidth: 360,
  undoAction: null,
};

export const app: { value: State } = $state({ value: { ...initial } });

let envelopeRequest = 0;
let messageRequest = 0;
let folderRefreshTimer: ReturnType<typeof setTimeout> | null = null;
let undoToken = 0;
let pendingUndo: {
  token: number;
  timer: ReturnType<typeof setTimeout>;
  restore: () => void;
  execute: () => Promise<void>;
} | null = null;
let composerLoadToken = 0;
let accountStatusRequest = 0;

type FolderView = {
  envelopes: Envelope[];
  envelopePage: number;
  hasMoreEnvelopes: boolean;
  selectedEnvelope: Envelope | null;
  selectedMessageIds: string[];
  selectionAnchorId: string | null;
  scrollTop: number;
};

const folderViews = new Map<string, FolderView>();
let folderScrollTop = 0;

export function recordFolderScroll(scrollTop: number) {
  folderScrollTop = scrollTop;
  const folderId = app.value.selectedFolderId;
  if (!folderId) return;
  const view = folderViews.get(folderId);
  if (view) view.scrollTop = scrollTop;
}

export function restoredFolderScroll(): number {
  const folderId = app.value.selectedFolderId;
  if (!folderId) return 0;
  return folderViews.get(folderId)?.scrollTop ?? 0;
}

function snapshotFolderView() {
  const folderId = app.value.selectedFolderId;
  if (!folderId) return;
  folderViews.set(folderId, {
    envelopes: app.value.envelopes,
    envelopePage: app.value.envelopePage,
    hasMoreEnvelopes: app.value.hasMoreEnvelopes,
    selectedEnvelope: app.value.selectedEnvelope,
    selectedMessageIds: app.value.selectedMessageIds,
    selectionAnchorId: app.value.selectionAnchorId,
    scrollTop: folderScrollTop,
  });
}

function patch(partial: Partial<State>) {
  Object.assign(app.value, partial);
}

function bounded(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(max, Math.max(min, Math.round(value)))
    : fallback;
}

function loadPreferences() {
  try {
    const saved = JSON.parse(localStorage.getItem("origami-preferences") ?? "{}") as Partial<State>;
    patch({
      theme: ["system", "light", "dark"].includes(saved.theme ?? "")
        ? saved.theme as State["theme"] : "system",
      density: ["comfortable", "compact"].includes(saved.density ?? "")
        ? saved.density as State["density"] : "comfortable",
      motion: ["system", "full", "reduced"].includes(saved.motion ?? "")
        ? saved.motion as State["motion"] : "system",
      layout: ["three-pane", "two-pane", "reading"].includes(saved.layout ?? "")
        ? saved.layout as WorkspaceLayout : "three-pane",
      sidebarWidth: bounded(saved.sidebarWidth, 260, 200, 420),
      threadListWidth: bounded(saved.threadListWidth, 360, 280, 520),
    });
  } catch {
    // Invalid preferences fall back to defaults.
  }
}

function savePreferences() {
  localStorage.setItem("origami-preferences", JSON.stringify({
    theme: app.value.theme,
    density: app.value.density,
    motion: app.value.motion,
    layout: app.value.layout,
    sidebarWidth: app.value.sidebarWidth,
    threadListWidth: app.value.threadListWidth,
  }));
}

export function setPreferences(preferences: Partial<Pick<State, "theme" | "density" | "motion">>) {
  patch(preferences);
  savePreferences();
}

export function setLayout(layout: WorkspaceLayout) {
  patch({ layout });
  savePreferences();
}

export function cycleLayout() {
  const layouts: WorkspaceLayout[] = ["three-pane", "two-pane", "reading"];
  const next = layouts[(layouts.indexOf(app.value.layout) + 1) % layouts.length];
  setLayout(next);
}

export function updatePaneWidth(kind: "sidebar" | "threadList", width: number) {
  if (kind === "sidebar") {
    patch({ sidebarWidth: bounded(width, app.value.sidebarWidth, 200, 420) });
  } else {
    patch({ threadListWidth: bounded(width, app.value.threadListWidth, 280, 520) });
  }
}

export function commitPaneWidth() {
  savePreferences();
}

function refreshFoldersSoon() {
  if (folderRefreshTimer) clearTimeout(folderRefreshTimer);
  folderRefreshTimer = setTimeout(async () => {
    folderRefreshTimer = null;
    try {
      const folders = await api.listFolders();
      patch({ folders });
      if (app.value.unifiedInbox) return;

      const selectedFolderStillExists = app.value.selectedFolderId
        ? folders.some((folder) => folder.id === app.value.selectedFolderId)
        : false;
      if (!selectedFolderStillExists) {
        const initialFolder = folders.find((folder) => folder.role === "Inbox") ?? folders[0];
        if (initialFolder) {
          await selectFolder(initialFolder.id);
        } else {
          patch({
            selectedFolderId: null,
            envelopes: [],
            selectedEnvelope: null,
            selectedMessageIds: [],
            selectionAnchorId: null,
            message: null,
          });
        }
      }
    } catch {
      // Sync event refreshes are best-effort; normal commands surface errors.
    }
  }, 100);
}

export async function bootstrap() {
  loadPreferences();
  try {
    const [accounts, folders, savedSearches, correspondents, accountStatuses] = await Promise.all([
      api.listAccounts(),
      api.listFolders(),
      api.listSavedSearches(),
      api.listCorrespondents(),
      api.accountStatuses(),
    ]);
    patch({
      accounts,
      folders,
      savedSearches,
      correspondents,
      accountStatuses,
      accountErrors: Object.fromEntries(
        Object.entries(accountStatuses)
          .filter(([, status]) => status.error)
          .map(([id, status]) => [id, status.error as string]),
      ),
      ready: true,
    });
    const initialFolder = folders.find((folder) => folder.role === "Inbox") ?? folders[0];
    if (initialFolder) await selectFolder(initialFolder.id);
  } catch (e) {
    patch({ ready: true, lastError: String(e) });
  }

  // Poll runtime status as recovery for lagged or missed events.
  pollAccountErrors();
  setInterval(pollAccountErrors, 5000);

  await onSyncEvent((event) => {
    const e = event as { type: string; folder?: string; accountId?: string };
    if (e.type === "folderSynced") {
      if (
        app.value.unifiedInbox
        && app.value.folders.some((folder) => folder.id === e.folder && folder.role === "Inbox")
      ) {
        void loadUnifiedInbox();
      }
      const selectedFolder = app.value.folders.find(
        (folder) => folder.id === app.value.selectedFolderId,
      );
      if (
        !app.value.searchQuery.trim()
        && e.folder
        && selectedFolder?.sourceIds.includes(e.folder)
      ) {
        void loadEnvelopes(selectedFolder.id);
      }
      refreshFoldersSoon();
    }
    if (
      e.type === "accountSyncStarted"
      || e.type === "error"
      || e.type === "accountSynced"
      || e.type === "outboxChanged"
    ) {
      pollAccountErrors();
    }
    if (e.type === "accountSynced" || e.type === "error") refreshFoldersSoon();
  });
}

export async function selectFolder(folderId: string) {
  if (app.value.selectedFolderId === folderId && !app.value.unifiedInbox) {
    await loadEnvelopes(folderId);
    return;
  }
  snapshotFolderView();
  messageRequest += 1;
  const cached = folderViews.get(folderId);
  folderScrollTop = cached?.scrollTop ?? 0;
  patch({
    selectedFolderId: folderId,
    unifiedInbox: false,
    searchQuery: "",
    searching: false,
    envelopes: cached?.envelopes ?? [],
    envelopePage: cached?.envelopePage ?? 1,
    hasMoreEnvelopes: cached?.hasMoreEnvelopes ?? true,
    selectingAll: false,
    selectedEnvelope: cached?.selectedEnvelope ?? null,
    selectedMessageIds: cached?.selectedMessageIds ?? [],
    selectionAnchorId: cached?.selectionAnchorId ?? null,
    message: cached?.selectedEnvelope?.id === app.value.message?.envelope.id
      ? app.value.message
      : null,
    messageLoading: false,
  });
  await loadEnvelopes(folderId);
  api.prefetchSelectedFolder(folderId).catch(() => {});
  if (cached?.selectedEnvelope) {
    const stillThere = app.value.envelopes.some((item) => item.id === cached.selectedEnvelope?.id);
    if (stillThere) void selectEnvelope(cached.selectedEnvelope);
  }
}

export async function selectUnifiedInbox() {
  messageRequest += 1;
  patch({
    selectedFolderId: null,
    unifiedInbox: true,
    searchQuery: "",
    envelopes: [],
    envelopePage: 1,
    hasMoreEnvelopes: true,
    selectingAll: false,
    selectedEnvelope: null,
    selectedMessageIds: [],
    selectionAnchorId: null,
    message: null,
    messageLoading: false,
  });
  await loadUnifiedInbox();
}

async function loadUnifiedInbox() {
  const request = ++envelopeRequest;
  patch({ envelopesLoading: true, envelopesLoadingMore: false });
  try {
    const envelopes = await api.listUnifiedInbox(1, 200);
    if (request !== envelopeRequest || !app.value.unifiedInbox) return;
    patch({
      envelopes,
      envelopesLoading: false,
      envelopePage: 1,
      hasMoreEnvelopes: envelopes.length === 200,
    });
  } catch (error) {
    if (request !== envelopeRequest) return;
    patch({ envelopesLoading: false, lastError: String(error) });
  }
}

export async function loadEnvelopes(folderId: string) {
  const request = ++envelopeRequest;
  patch({
    envelopesLoading: app.value.envelopes.length === 0,
    envelopesLoadingMore: false,
  });
  try {
    const envelopes = await api.listEnvelopes(folderId, 1, 200);
    if (request !== envelopeRequest || app.value.selectedFolderId !== folderId) return;
    const available = new Set(envelopes.map((envelope) => envelope.id));
    patch({
      envelopes,
      envelopesLoading: false,
      envelopePage: 1,
      hasMoreEnvelopes: envelopes.length === 200,
      selectedMessageIds: app.value.selectedMessageIds.filter((id) => available.has(id)),
    });
  } catch (e) {
    if (request !== envelopeRequest || app.value.selectedFolderId !== folderId) return;
    patch({ envelopesLoading: false, lastError: String(e) });
  }
}

export async function loadMoreEnvelopes() {
  const folderId = app.value.selectedFolderId;
  const query = app.value.searchQuery.trim();
  if (query) {
    if (
      app.value.envelopesLoading
      || app.value.envelopesLoadingMore
      || !app.value.hasMoreEnvelopes
    ) return;

    const request = envelopeRequest;
    const page = app.value.envelopePage + 1;
    patch({ envelopesLoadingMore: true });
    try {
      const next = await api.searchPage(query, page, 200);
      if (request !== envelopeRequest || app.value.searchQuery.trim() !== query) {
        if (request === envelopeRequest) patch({ envelopesLoadingMore: false });
        return;
      }
      const known = new Set(app.value.envelopes.map((envelope) => envelope.id));
      patch({
        envelopes: [...app.value.envelopes, ...next.filter((envelope) => !known.has(envelope.id))],
        envelopesLoadingMore: false,
        envelopePage: page,
        hasMoreEnvelopes: next.length === 200,
      });
    } catch (error) {
      if (request !== envelopeRequest || app.value.searchQuery.trim() !== query) {
        if (request === envelopeRequest) patch({ envelopesLoadingMore: false });
        return;
      }
      patch({ envelopesLoadingMore: false, lastError: String(error) });
    }
    return;
  }

  if (
    (!folderId && !app.value.unifiedInbox)
    || app.value.envelopesLoading
    || app.value.envelopesLoadingMore
    || !app.value.hasMoreEnvelopes
  ) return;

  const request = envelopeRequest;
  const page = app.value.envelopePage + 1;
  patch({ envelopesLoadingMore: true });
  try {
    const next = app.value.unifiedInbox
      ? await api.listUnifiedInbox(page, 200)
      : await api.listEnvelopes(folderId!, page, 200);
    if (request !== envelopeRequest || (!app.value.unifiedInbox && app.value.selectedFolderId !== folderId)) {
      if (request === envelopeRequest) patch({ envelopesLoadingMore: false });
      return;
    }
    const known = new Set(app.value.envelopes.map((envelope) => envelope.id));
    patch({
      envelopes: [...app.value.envelopes, ...next.filter((envelope) => !known.has(envelope.id))],
      envelopesLoadingMore: false,
      envelopePage: page,
      hasMoreEnvelopes: next.length === 200,
    });
  } catch (error) {
    if (request !== envelopeRequest || app.value.selectedFolderId !== folderId) {
      if (request === envelopeRequest) patch({ envelopesLoadingMore: false });
      return;
    }
    patch({ envelopesLoadingMore: false, lastError: String(error) });
  }
}

export async function selectEnvelope(envelope: Envelope) {
  const sources = envelopeSources(envelope);
  const primary = sources.find((source) =>
    source.mailboxId === envelope.mailboxId && source.serverUid === envelope.serverUid,
  ) ?? sources[0];
  if (!primary) return;
  const request = ++messageRequest;
  patch({ selectedEnvelope: envelope, messageLoading: true });
  try {
    let message: MessageDto | null = null;
    let loadedSource: EnvelopeSource | null = null;
    let lastError: unknown = null;
    const orderedSources = [primary, ...sources.filter((item) => item !== primary)];
    for (const source of orderedSources) {
      try {
        const cached = await api.getCachedMessage(source.mailboxId, source.serverUid);
        if (cached) {
          message = cached;
          loadedSource = source;
          break;
        }
      } catch {
        // Cache misses stay local; get_message handles IMAP.
      }
    }
    if (!message) {
      if (app.value.message?.envelope.id !== envelope.id) patch({ message: null });
      for (const source of orderedSources) {
        try {
          message = await api.getMessage(source.mailboxId, source.serverUid);
          loadedSource = source;
          break;
        } catch (error) {
          lastError = error;
        }
      }
    }
    if (!message) throw lastError ?? new Error("message not found");
    if (
      request !== messageRequest
      || app.value.selectedEnvelope?.id !== envelope.id
    ) return;
    const attachmentState = message.attachments.length > 0;
    const messageWithAttachmentState = {
      ...message,
      envelope: {
        ...message.envelope,
        ...envelope,
        mailboxId: (loadedSource ?? primary).mailboxId,
        serverUid: (loadedSource ?? primary).serverUid,
        sources,
        hasAttachment: attachmentState,
      },
    };
    patch({
      message: messageWithAttachmentState,
      messageLoading: false,
      envelopes: app.value.envelopes.map((item) =>
        item.id === envelope.id ? { ...item, hasAttachment: attachmentState } : item
      ),
    });
    // First-open marks as read: optimistic local flip + command.
    if (!messageWithAttachmentState.envelope.flags.includes("Seen")) {
      const next: Flag[] = Array.from(new Set([...messageWithAttachmentState.envelope.flags, "Seen"]));
      const envelopes = app.value.envelopes.map((item) =>
        item.id === envelope.id ? { ...item, flags: next } : item
      );
      patch({
        envelopes,
        message: {
          ...messageWithAttachmentState,
          envelope: { ...messageWithAttachmentState.envelope, flags: next },
        },
      });
      try {
        await Promise.all(
          sources.map((source) => api.storeFlags(source.mailboxId, source.serverUid, next)),
        );
        await refreshAfterMessageAction();
      } catch (e) {
        patch({ lastError: String(e) });
      }
    }
  } catch (e) {
    if (request !== messageRequest) return;
    patch({ messageLoading: false, lastError: String(e) });
  }
}

export async function searchMessages(query: string) {
  const normalized = query.trim();
  patch({
    searchQuery: query,
    selectedMessageIds: [],
    selectionAnchorId: null,
    selectingAll: false,
  });
  if (!normalized) {
    if (app.value.unifiedInbox) await loadUnifiedInbox();
    else if (app.value.selectedFolderId) await loadEnvelopes(app.value.selectedFolderId);
    return;
  }

  const request = ++envelopeRequest;
  patch({
    searching: true,
    envelopesLoading: true,
    envelopesLoadingMore: false,
    selectedEnvelope: null,
    message: null,
  });
  try {
    const envelopes = await api.searchPage(normalized, 1, 200);
    if (request !== envelopeRequest || app.value.searchQuery.trim() !== normalized) return;
    patch({
      envelopes,
      searching: false,
      envelopesLoading: false,
      envelopePage: 1,
      hasMoreEnvelopes: envelopes.length === 200,
    });
  } catch (error) {
    if (request !== envelopeRequest) return;
    patch({ searching: false, envelopesLoading: false, lastError: String(error) });
  }
}

export async function saveCurrentSearch(name: string) {
  const query = app.value.searchQuery.trim();
  if (!name.trim() || !query) return;
  try {
    const saved = await api.saveSearch(name, query);
    patch({ savedSearches: [...app.value.savedSearches, saved] });
  } catch (error) {
    patch({ lastError: String(error) });
  }
}

export async function removeSavedSearch(id: string) {
  try {
    await api.deleteSavedSearch(id);
    patch({ savedSearches: app.value.savedSearches.filter((saved) => saved.id !== id) });
  } catch (error) {
    patch({ lastError: String(error) });
  }
}

export function selectEnvelopeExclusive(envelope: Envelope) {
  patch({ selectedMessageIds: [envelope.id], selectionAnchorId: envelope.id });
  return selectEnvelope(envelope);
}

function envelopeSources(envelope: Envelope): EnvelopeSource[] {
  const sources = envelope.sources?.length
    ? envelope.sources
    : envelope.mailboxId && envelope.serverUid != null
      ? [{ mailboxId: envelope.mailboxId, serverUid: envelope.serverUid }]
      : [];
  const unique = new Map<string, EnvelopeSource>();
  for (const source of sources) {
    unique.set(`${source.mailboxId}:${source.serverUid}`, source);
  }
  return [...unique.values()];
}

function folderForSource(sourceId: string): Mailbox | undefined {
  return app.value.folders.find(
    (folder) => folder.id === sourceId || folder.sourceIds.includes(sourceId),
  );
}

export function toggleEnvelopeSelection(envelope: Envelope, extendRange = false) {
  const selected = new Set(app.value.selectedMessageIds);
  if (extendRange && app.value.selectionAnchorId) {
    const start = app.value.envelopes.findIndex((item) => item.id === app.value.selectionAnchorId);
    const end = app.value.envelopes.findIndex((item) => item.id === envelope.id);
    if (start >= 0 && end >= 0) {
      const [from, to] = start < end ? [start, end] : [end, start];
      for (const item of app.value.envelopes.slice(from, to + 1)) selected.add(item.id);
    }
  } else if (selected.has(envelope.id)) {
    selected.delete(envelope.id);
  } else {
    selected.add(envelope.id);
  }
  patch({
    selectedMessageIds: [...selected],
    selectionAnchorId: extendRange ? app.value.selectionAnchorId : envelope.id,
  });
}

export function clearMessageSelection() {
  patch({ selectedMessageIds: [], selectionAnchorId: null });
}

export function clearMessageView() {
  messageRequest += 1;
  patch({ selectedEnvelope: null, message: null, messageLoading: false });
}

export function selectAllLoadedMessages() {
  patch({
    selectedMessageIds: app.value.envelopes.map((envelope) => envelope.id),
    selectionAnchorId: app.value.envelopes[0]?.id ?? null,
  });
}

/// Load every locally available page before selecting, so Ctrl/Cmd+A and the
/// bulk toolbar do not silently exclude messages beyond the current viewport.
export async function selectAllMessages() {
  if (app.value.selectingAll) return;
  const context = {
    folderId: app.value.selectedFolderId,
    unifiedInbox: app.value.unifiedInbox,
    query: app.value.searchQuery,
  };
  const sameContext = () => app.value.selectedFolderId === context.folderId
    && app.value.unifiedInbox === context.unifiedInbox
    && app.value.searchQuery === context.query;

  patch({ selectingAll: true });
  try {
    while (sameContext() && app.value.hasMoreEnvelopes) {
      const beforePage = app.value.envelopePage;
      const beforeCount = app.value.envelopes.length;
      await loadMoreEnvelopes();
      if (
        !sameContext()
        || (app.value.envelopePage === beforePage
          && app.value.envelopes.length === beforeCount
          && app.value.hasMoreEnvelopes)
      ) break;
    }
    if (sameContext()) selectAllLoadedMessages();
  } finally {
    if (sameContext()) patch({ selectingAll: false });
  }
}

export async function setSelectedFlag(flag: Flag, enabled: boolean) {
  const selected = new Set(app.value.selectedMessageIds);
  if (selected.size === 0) return;

  const targets = app.value.envelopes.filter(
    (envelope) => selected.has(envelope.id) && envelopeSources(envelope).length > 0,
  );
  const nextFlags = (envelope: Envelope): Flag[] => enabled
    ? Array.from(new Set([...envelope.flags, flag]))
    : envelope.flags.filter((item) => item !== flag);
  const updatesByFolder = new Map<string, { serverUid: number; flags: Flag[] }[]>();
  for (const envelope of targets) {
    for (const source of envelopeSources(envelope)) {
      const updates = updatesByFolder.get(source.mailboxId) ?? [];
      updates.push({ serverUid: source.serverUid, flags: nextFlags(envelope) });
      updatesByFolder.set(source.mailboxId, updates);
    }
  }
  const envelopes = app.value.envelopes.map((envelope) =>
    selected.has(envelope.id) ? { ...envelope, flags: nextFlags(envelope) } : envelope
  );
  const message = app.value.message && selected.has(app.value.message.envelope.id)
    ? {
        ...app.value.message,
        envelope: {
          ...app.value.message.envelope,
          flags: nextFlags(app.value.message.envelope),
        },
      }
    : app.value.message;
  patch({ envelopes, message });

  try {
    await Promise.all(
      [...updatesByFolder].map(([folderId, folderUpdates]) => api.storeFlagsBatch(folderId, folderUpdates)),
    );
    await refreshAfterMessageAction();
  } catch (error) {
    patch({ lastError: String(error) });
    if (app.value.searchQuery.trim()) {
      await searchMessages(app.value.searchQuery);
    } else if (app.value.selectedFolderId) {
      await loadEnvelopes(app.value.selectedFolderId);
    }
  }
}

export async function setSelectedKeyword(rawKeyword: string, enabled: boolean) {
  const keyword = rawKeyword.trim().replaceAll(/\s+/g, "-");
  const selected = new Set(app.value.selectedMessageIds);
  if (!keyword || selected.size === 0) return;
  const targets = selectedTargets();
  const nextKeywords = (envelope: Envelope): string[] => enabled
    ? Array.from(new Set([...envelope.keywords, keyword]))
    : envelope.keywords.filter((item) => item !== keyword);
  const envelopes = app.value.envelopes.map((envelope) =>
    selected.has(envelope.id) ? { ...envelope, keywords: nextKeywords(envelope) } : envelope
  );
  const message = app.value.message && selected.has(app.value.message.envelope.id)
    ? {
        ...app.value.message,
        envelope: {
          ...app.value.message.envelope,
          keywords: nextKeywords(app.value.message.envelope),
        },
      }
    : app.value.message;
  patch({ envelopes, message });

  const byFolder = new Map<string, {
    serverUid: number;
    flags: Flag[];
    keywords: string[];
  }[]>();
  for (const target of targets) {
    for (const source of envelopeSources(target)) {
      const updates = byFolder.get(source.mailboxId) ?? [];
      updates.push({
        serverUid: source.serverUid,
        flags: target.flags,
        keywords: nextKeywords(target),
      });
      byFolder.set(source.mailboxId, updates);
    }
  }
  try {
    await Promise.all(
      [...byFolder].map(([folderId, updates]) => api.storeKeywordsBatch(folderId, updates)),
    );
    await refreshAfterMessageAction();
  } catch (error) {
    patch({ lastError: String(error) });
    if (app.value.searchQuery.trim()) await searchMessages(app.value.searchQuery);
    else if (app.value.selectedFolderId) await loadEnvelopes(app.value.selectedFolderId);
  }
}

function removeSelectedLocally(targets: Envelope[]) {
  const removed = new Set(targets.map((target) => target.id));
  const folderChanges = new Map<string, { total: number; unread: number }>();
  for (const target of targets) {
    const displayedFolderIds = new Set<string>();
    for (const source of envelopeSources(target)) {
      const displayedFolderId = folderForSource(source.mailboxId)?.id ?? source.mailboxId;
      displayedFolderIds.add(displayedFolderId);
    }
    for (const displayedFolderId of displayedFolderIds) {
      const change = folderChanges.get(displayedFolderId) ?? { total: 0, unread: 0 };
      change.total -= 1;
      if (!target.flags.includes("Seen")) change.unread -= 1;
      folderChanges.set(displayedFolderId, change);
    }
  }
  patch({
    envelopes: app.value.envelopes.filter((envelope) => !removed.has(envelope.id)),
    folders: app.value.folders.map((folder) => {
      const change = folderChanges.get(folder.id);
      return change ? {
        ...folder,
        total: Math.max(0, folder.total + change.total),
        unread: Math.max(0, folder.unread + change.unread),
      } : folder;
    }),
    selectedMessageIds: [],
    selectionAnchorId: null,
    selectedEnvelope: app.value.selectedEnvelope && removed.has(app.value.selectedEnvelope.id)
      ? null : app.value.selectedEnvelope,
    message: app.value.message && removed.has(app.value.message.envelope.id)
      ? null : app.value.message,
  });
}

function scheduleMessageAction(message: string, targets: Envelope[], execute: () => Promise<void>) {
  if (pendingUndo) {
    clearTimeout(pendingUndo.timer);
    void pendingUndo.execute();
    pendingUndo = null;
  }
  const snapshot = {
    folderId: app.value.selectedFolderId,
    searchQuery: app.value.searchQuery,
    envelopes: app.value.envelopes,
    folders: app.value.folders,
    selectedMessageIds: app.value.selectedMessageIds,
    selectionAnchorId: app.value.selectionAnchorId,
    selectedEnvelope: app.value.selectedEnvelope,
    currentMessage: app.value.message,
  };
  const token = ++undoToken;
  const restore = () => {
    if (
      app.value.selectedFolderId === snapshot.folderId
      && app.value.searchQuery === snapshot.searchQuery
    ) {
      patch({
        envelopes: snapshot.envelopes,
        folders: snapshot.folders,
        selectedMessageIds: snapshot.selectedMessageIds,
        selectionAnchorId: snapshot.selectionAnchorId,
        selectedEnvelope: snapshot.selectedEnvelope,
        message: snapshot.currentMessage,
      });
    }
  };
  const run = async () => {
    if (pendingUndo?.token === token) pendingUndo = null;
    if (app.value.undoAction?.token === token) patch({ undoAction: null });
    await execute();
  };
  removeSelectedLocally(targets);
  const timer = setTimeout(() => void run(), 6000);
  pendingUndo = { token, timer, restore, execute: run };
  patch({ undoAction: { message, token } });
}

export function undoLastMessageAction() {
  if (!pendingUndo) return;
  clearTimeout(pendingUndo.timer);
  pendingUndo.restore();
  pendingUndo = null;
  patch({ undoAction: null });
}

function selectedTargets(): Envelope[] {
  const selected = new Set(app.value.selectedMessageIds);
  return app.value.envelopes.filter(
    (envelope) => selected.has(envelope.id) && envelopeSources(envelope).length > 0,
  );
}

async function refreshAfterMessageAction() {
  try {
    patch({ folders: await api.listFolders() });
  } catch {
    // The local optimistic state remains authoritative until next sync.
  }
}

export async function moveSelectedToFolder(destinationFolderId: string) {
  if (!destinationFolderId) return;
  const targets = selectedTargets();
  if (targets.length === 0) return;
  const destination = app.value.folders.find((folder) => folder.id === destinationFolderId);
  if (!destination) return;

  const destinationSources = new Set([destination.id, ...destination.sourceIds]);
  const byFolder = new Map<string, Set<number>>();
  for (const target of targets) {
    for (const source of envelopeSources(target)) {
      if (destinationSources.has(source.mailboxId)) continue;
      const sourceFolder = folderForSource(source.mailboxId);
      if (!sourceFolder || sourceFolder.accountId !== destination.accountId) {
        patch({ lastError: "Messages cannot be moved between accounts" });
        return;
      }
      const group = byFolder.get(source.mailboxId) ?? new Set<number>();
      group.add(source.serverUid);
      byFolder.set(source.mailboxId, group);
    }
  }
  if (byFolder.size === 0) return;

  scheduleMessageAction(`Moved ${targets.length} message${targets.length === 1 ? "" : "s"}`, targets, async () => {
    try {
      await Promise.all([...byFolder].map(([folderId, serverUids]) =>
        api.moveMessages(folderId, destinationFolderId, [...serverUids])
      ));
      await refreshAfterMessageAction();
    } catch (error) {
      patch({ lastError: String(error) });
      if (app.value.searchQuery.trim()) await searchMessages(app.value.searchQuery);
      else if (app.value.selectedFolderId) await loadEnvelopes(app.value.selectedFolderId);
    }
  });
}

export async function moveSelectedToRole(role: Extract<MailboxRole, "Archive" | "Trash" | "Junk">) {
  const targets = selectedTargets();
  if (targets.length === 0) return;
  const byFolder = new Map<string, Set<number>>();
  for (const target of targets) {
    for (const source of envelopeSources(target)) {
      const group = byFolder.get(source.mailboxId) ?? new Set<number>();
      group.add(source.serverUid);
      byFolder.set(source.mailboxId, group);
    }
  }
  const moves: { sourceId: string; destinationId: string; serverUids: number[] }[] = [];
  for (const [sourceId, serverUids] of byFolder) {
    const source = folderForSource(sourceId);
    const destination = app.value.folders.find(
      (folder) => folder.accountId === source?.accountId && folder.role === role,
    );
    if (!destination) {
      patch({ lastError: `${role} folder not found for ${source?.name ?? "this account"}` });
      return;
    }
    if (destination.id !== sourceId && !destination.sourceIds.includes(sourceId)) {
      moves.push({ sourceId, destinationId: destination.id, serverUids: [...serverUids] });
    }
  }
  if (moves.length === 0) return;

  scheduleMessageAction(`${role} ${targets.length} message${targets.length === 1 ? "" : "s"}`, targets, async () => {
    try {
      await Promise.all(moves.map((move) => api.moveMessages(
        move.sourceId,
        move.destinationId,
        move.serverUids,
      )));
      await refreshAfterMessageAction();
    } catch (error) {
      patch({ lastError: String(error) });
      if (app.value.searchQuery.trim()) await searchMessages(app.value.searchQuery);
      else if (app.value.selectedFolderId) await loadEnvelopes(app.value.selectedFolderId);
    }
  });
}

export async function deleteSelectedPermanently() {
  const targets = selectedTargets();
  if (targets.length === 0) return;
  const byFolder = new Map<string, Set<number>>();
  for (const target of targets) {
    for (const source of envelopeSources(target)) {
      const group = byFolder.get(source.mailboxId) ?? new Set<number>();
      group.add(source.serverUid);
      byFolder.set(source.mailboxId, group);
    }
  }

  scheduleMessageAction(`Deleted ${targets.length} message${targets.length === 1 ? "" : "s"}`, targets, async () => {
    try {
      await Promise.all([...byFolder].map(([folderId, serverUids]) =>
        api.deleteMessages(folderId, [...serverUids])
      ));
      await refreshAfterMessageAction();
    } catch (error) {
      patch({ lastError: String(error) });
      if (app.value.searchQuery.trim()) await searchMessages(app.value.searchQuery);
      else if (app.value.selectedFolderId) await loadEnvelopes(app.value.selectedFolderId);
    }
  });
}

function blankDraft() {
  return {
    to: "",
    cc: "",
    bcc: "",
    subject: "",
    html: "<p></p>",
  };
}

function blankThreading(): State["composerThreading"] {
  return { inReplyTo: null, references: [] };
}

function legacySavedDraft(): State["composerDraft"] | null {
  try {
    const raw = localStorage.getItem("origami-composer-draft");
    return raw ? JSON.parse(raw) as State["composerDraft"] : null;
  } catch {
    return null;
  }
}

export async function openComposer(
  draft?: Partial<State["composerDraft"]>,
  threading?: Partial<State["composerThreading"]>,
) {
  if (app.value.accounts.length === 0) {
    patch({ lastNotice: "Add an account before composing a message." });
    return;
  }
  const folder = app.value.folders.find((item) => item.id === app.value.selectedFolderId);
  const account = app.value.accounts.find((item) => item.dbId === folder?.accountId)
    ?? app.value.accounts[0];
  const loadToken = ++composerLoadToken;
  const legacy = draft ? null : legacySavedDraft();
  let recovered: Awaited<ReturnType<typeof api.loadComposerDraft>> = null;
  if (!draft && !legacy) {
    try {
      recovered = await api.loadComposerDraft();
    } catch {
      // A missing draft must never prevent opening the composer.
    }
  }
  if (loadToken !== composerLoadToken) return;
  patch({
    composerOpen: true,
    composerAccountId: recovered?.accountId ?? account?.id ?? null,
    composerAttachments: recovered?.attachments ?? [],
    composerThreading: {
      ...blankThreading(),
      ...(recovered?.threading ?? {}),
      ...(threading ?? {}),
    },
    composerDraft: {
      ...blankDraft(),
      ...(draft ?? legacy ?? recovered?.draft ?? {}),
    },
  });
}

function htmlEscape(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function openReplyComposer(mode: "reply" | "replyAll" | "forward") {
  const message = app.value.message;
  if (!message) return;
  const accountEmails = new Set(app.value.accounts.map((account) => account.email.toLowerCase()));
  const sender = message.envelope.from[0]?.addr ?? "";
  const replyTarget = message.headers.replyTo[0]?.addr ?? sender;
  const recipients = mode === "replyAll"
    ? [replyTarget, ...message.envelope.to.map((address) => address.addr), ...message.headers.cc.map((address) => address.addr)]
        .filter((address, index, all) =>
          address && !accountEmails.has(address.toLowerCase()) && all.indexOf(address) === index
        )
    : mode === "reply" ? [replyTarget].filter(Boolean) : [];
  const prefix = mode === "forward" ? "Fwd:" : "Re:";
  const subject = new RegExp(`^${prefix.replace(":", "")}:`, "i").test(message.envelope.subject)
    ? message.envelope.subject
    : `${prefix} ${message.envelope.subject}`;
  const quoted = htmlEscape(message.text ?? message.envelope.subject);
  const author = htmlEscape((message.envelope.from[0]?.name ?? sender) || "the sender");
  const attribution = htmlEscape(`On ${message.envelope.date ?? "an earlier date"}, ${author} wrote:`);
  openComposer({
    to: recipients.join(", "),
    subject,
    html: `<p></p><p>${attribution}</p><blockquote><p>${quoted.replaceAll("\n", "<br>")}</p></blockquote>`,
  }, mode === "forward" ? undefined : {
    inReplyTo: message.headers.messageId,
    references: [...message.headers.references, ...(message.headers.messageId ? [message.headers.messageId] : [])],
  });
}

export async function closeComposer() {
  try {
    await api.saveComposerDraft({
      accountId: app.value.composerAccountId,
      draft: app.value.composerDraft,
      attachments: app.value.composerAttachments,
      threading: app.value.composerThreading,
    });
  } catch {
    patch({ lastNotice: "Draft could not be saved" });
  }
  patch({ composerOpen: false });
  api.syncComposerDraft().catch(() => {
    patch({ lastNotice: "Draft saved locally; server Drafts sync will retry next time" });
  });
}

export async function sendComposer() {
  if (app.value.sending) return;
  const account = app.value.accounts.find((item) => item.id === app.value.composerAccountId)
    ?? app.value.accounts[0];
  if (!account) {
    patch({ lastError: "no account configured" });
    return;
  }
  const draft = app.value.composerDraft;
  const to = draft.to.split(/[,\s]+/).filter(Boolean);
  const cc = draft.cc.split(/[,\s]+/).filter(Boolean);
  const bcc = draft.bcc.split(/[,\s]+/).filter(Boolean);
  if (to.length === 0 && cc.length === 0 && bcc.length === 0) {
    patch({ lastError: "add at least one recipient" });
    return;
  }
  patch({ sending: true });
  try {
    const result = await api.sendMessage({
      fromName: account.name,
      fromAddr: account.email,
      to,
      cc,
      bcc,
      subject: draft.subject,
      html: draft.html,
      inReplyTo: app.value.composerThreading.inReplyTo,
      references: app.value.composerThreading.references,
      attachments: app.value.composerAttachments.map(({ name, mime, dataBase64 }) => ({
        name,
        mime,
        dataBase64,
      })),
    });
    localStorage.removeItem("origami-composer-draft");
    await api.deleteComposerDraft();
    patch({ composerOpen: false });
    patch({
      lastError: null,
      lastNotice: result.queued ? "Message queued and will send when the account reconnects" : "Message sent",
    });
  } catch (e) {
    patch({ lastError: String(e) });
  } finally {
    patch({ sending: false });
  }
}

export async function syncNow(accountId?: string) {
  patch({ syncing: true });
  try {
    await api.syncNow(accountId);
    const folders = await api.listFolders();
    const selectedFolder = folders.find((folder) => folder.id === app.value.selectedFolderId);
    if (app.value.unifiedInbox) {
      patch({ folders });
      await loadUnifiedInbox();
    } else if (selectedFolder) {
      const envs = await api.listEnvelopes(selectedFolder.id, 1, 200);
      patch({ envelopes: envs, folders });
    } else {
      patch({ folders, selectedFolderId: null, envelopes: [], selectedEnvelope: null, message: null });
      const initialFolder = folders.find((folder) => folder.role === "Inbox") ?? folders[0];
      if (initialFolder) await selectFolder(initialFolder.id);
    }
  } catch (e) {
    patch({ lastError: String(e) });
  } finally {
    patch({ syncing: false });
  }
}

export async function removeAccount(accountId: string) {
  try {
    const accounts = await api.removeAccount(accountId);
    const folders = await api.listFolders();
    patch({
      accounts,
      folders,
      selectedFolderId: null,
      envelopes: [],
      selectedMessageIds: [],
      selectionAnchorId: null,
      message: null,
    });
    const initialFolder = folders.find((folder) => folder.role === "Inbox") ?? folders[0];
    if (initialFolder) await selectFolder(initialFolder.id);
  } catch (e) {
    patch({ lastError: String(e) });
  }
}

export async function pollAccountErrors() {
  const request = ++accountStatusRequest;
  try {
    const statuses = await api.accountStatuses();
    if (request !== accountStatusRequest) return;
    const errors = Object.fromEntries(
      Object.entries(statuses)
        .filter(([, status]) => status.error)
        .map(([id, status]) => [id, status.error as string]),
    );
    patch({ accountStatuses: statuses, accountErrors: errors });
  } catch {
    // Polling is best-effort.
  }
}

export async function reconnectAccount(accountId: string) {
  patch({ syncing: true });
  try {
    await api.syncNow(accountId);
    patch({ lastError: null });
  } catch (e) {
    patch({ lastError: String(e) });
  } finally {
    patch({ syncing: false });
    await pollAccountErrors();
  }
}
