import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    getCachedMessage: vi.fn(),
    getMessage: vi.fn(),
    storeFlags: vi.fn().mockResolvedValue(undefined),
    storeFlagsBatch: vi.fn().mockResolvedValue(undefined),
    listFolders: vi.fn().mockResolvedValue([]),
    listEnvelopes: vi.fn().mockResolvedValue([]),
    listUnifiedInbox: vi.fn().mockResolvedValue([]),
    searchPage: vi.fn().mockResolvedValue([]),
    searchCount: vi.fn().mockResolvedValue(0),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
    moveMessages: vi.fn().mockResolvedValue(undefined),
    deleteMessages: vi.fn().mockResolvedValue(undefined),
    threadEnvelopes: vi.fn().mockResolvedValue([]),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import type { MessageDto } from "./api";
import {
  app,
  moveSelectedToRole,
  undoLastMessageAction,
} from "./stores.svelte";
import type { Envelope, Mailbox } from "./types";

function stubLocalStorage() {
  const values = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
      clear: () => values.clear(),
    },
    configurable: true,
  });
}

function env(id: string, flags: Envelope["flags"] = []): Envelope {
  return {
    id,
    mailboxId: "inbox",
    subject: id,
    from: [],
    to: [],
    date: null,
    receivedAt: null,
    flags,
    keywords: [],
    hasAttachment: false,
    size: 10,
    serverUid: id.length,
    messageId: `<${id}@example.org>`,
    threadId: `<${id}@example.org>`,
    sources: [{ mailboxId: "inbox", serverUid: id.length }],
  };
}

function folder(id: string, role: Mailbox["role"], extra: Partial<Mailbox> = {}): Mailbox {
  return {
    id,
    accountId: "db-1",
    name: id,
    role,
    subscribed: true,
    total: 0,
    unread: 0,
    sourceIds: [],
    ...extra,
  };
}

function message(id: string): MessageDto {
  return {
    envelope: env(id),
    text: "body",
    html: null,
    headers: {
      cc: [],
      replyTo: [],
      sender: [],
      messageId: null,
      inReplyTo: [],
      references: [],
      listUnsubscribe: [],
      authenticationResults: null,
    },
    attachments: [],
    parts: [],
    parseWarnings: [],
  };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("auto-advance after destructive actions", () => {
  beforeEach(() => {
    stubLocalStorage();
    vi.useFakeTimers();
    vi.clearAllMocks();
    undoLastMessageAction();
    app.value.accounts = [
      { id: "account-1", dbId: "db-1", name: "Work", email: "me@example.org", hasImap: true, hasSmtp: true },
    ] as never;
    app.value.folders = [
      folder("inbox", "Inbox", { total: 3, unread: 3 }),
      folder("archive", "Archive"),
    ];
    app.value.selectedFolderId = "inbox";
    app.value.unifiedInbox = false;
    app.value.unreadOnly = false;
    app.value.searchQuery = "";
    app.value.markReadDelay = -1;
    app.value.hasMoreEnvelopes = false;
    app.value.envelopesLoading = false;
    app.value.envelopesLoadingMore = false;
    app.value.envelopePage = 1;
    app.value.message = null;
    app.value.selectedEnvelope = null;
    app.value.selectedMessageIds = [];
    app.value.selectionAnchorId = null;
    app.value.undoAction = null;
    vi.mocked(api.getCachedMessage).mockImplementation(async (_folderId, serverUid) =>
      message(serverUid === env("page2").serverUid ? "page2" : "row"),
    );
    vi.mocked(api.listEnvelopes).mockResolvedValue([]);
    vi.mocked(api.listFolders).mockResolvedValue(app.value.folders);
    vi.mocked(api.moveMessages).mockResolvedValue(undefined);
  });

  afterEach(() => {
    undoLastMessageAction();
    vi.useRealTimers();
  });

  it("archive_selects_the_slot_next_remaining_row", async () => {
    app.value.envelopes = [env("a"), env("b"), env("c")];
    app.value.selectedEnvelope = env("a");
    app.value.selectedMessageIds = ["a"];
    app.value.hasMoreEnvelopes = false;
    await moveSelectedToRole("Archive");
    expect(app.value.envelopes.map((e) => e.id)).toEqual(["b", "c"]);
    expect(app.value.selectedEnvelope?.id).toBe("b");
    expect(app.value.selectedMessageIds).toEqual(["b"]);
  });

  it("archive_last_row_selects_previous", async () => {
    app.value.envelopes = [env("a"), env("b")];
    app.value.selectedEnvelope = env("b");
    app.value.selectedMessageIds = ["b"];
    await moveSelectedToRole("Archive");
    expect(app.value.envelopes.map((e) => e.id)).toEqual(["a"]);
    expect(app.value.selectedEnvelope?.id).toBe("a");
    expect(app.value.selectedMessageIds).toEqual(["a"]);
  });

  it("archive_last_remaining_row_empties_the_pane", async () => {
    app.value.envelopes = [env("a")];
    app.value.selectedEnvelope = env("a");
    app.value.selectedMessageIds = ["a"];
    app.value.message = message("a");
    app.value.hasMoreEnvelopes = false;
    await moveSelectedToRole("Archive");
    expect(app.value.envelopes).toEqual([]);
    expect(app.value.selectedEnvelope).toBeNull();
    expect(app.value.message).toBeNull();
    expect(app.value.selectedMessageIds).toEqual([]);
  });

  it("unread_only_does_not_keep_the_archived_id", async () => {
    app.value.unreadOnly = true;
    app.value.envelopes = [env("a"), env("b")];
    app.value.selectedEnvelope = env("a");
    app.value.selectedMessageIds = ["a"];
    await moveSelectedToRole("Archive");
    expect(app.value.envelopes.map((e) => e.id)).toEqual(["b"]);
    expect(app.value.envelopes.some((e) => e.id === "a")).toBe(false);
  });

  it("keep_open_when_open_row_was_not_a_target", async () => {
    app.value.envelopes = [env("a"), env("b"), env("c")];
    app.value.selectedEnvelope = env("c");
    app.value.selectedMessageIds = ["a"];
    await moveSelectedToRole("Archive");
    expect(app.value.selectedEnvelope?.id).toBe("c");
    expect(app.value.selectedMessageIds).toEqual(["c"]);
  });

  it("null_open_row_with_selected_ids_does_not_throw", async () => {
    app.value.envelopes = [env("a"), env("b")];
    app.value.selectedEnvelope = null;
    app.value.selectedMessageIds = ["a", "b"];
    await expect(moveSelectedToRole("Archive")).resolves.toBeUndefined();
    expect(app.value.envelopes).toEqual([]);
  });

  it("empty_page_load_more_selects_first_new_row", async () => {
    app.value.envelopes = [env("only")];
    app.value.selectedEnvelope = env("only");
    app.value.selectedMessageIds = ["only"];
    app.value.hasMoreEnvelopes = true;
    app.value.selectedFolderId = "inbox";
    vi.mocked(api.listEnvelopes).mockResolvedValue([env("page2")]);
    await moveSelectedToRole("Archive");
    await flush();
    expect(app.value.selectedEnvelope?.id).toBe("page2");
    expect(app.value.selectedMessageIds).toEqual(["page2"]);
  });

  it("undo_restores_envelopes_and_selection_and_drops_inflight_loadMore", async () => {
    app.value.envelopes = [env("only")];
    app.value.selectedEnvelope = env("only");
    app.value.selectedMessageIds = ["only"];
    app.value.hasMoreEnvelopes = true;
    app.value.selectedFolderId = "inbox";
    let resolvePage: (rows: Envelope[]) => void = () => {};
    vi.mocked(api.listEnvelopes).mockImplementation(
      () => new Promise((resolve) => { resolvePage = resolve; }),
    );
    await moveSelectedToRole("Archive");
    expect(app.value.envelopes).toEqual([]);
    undoLastMessageAction();
    expect(app.value.envelopes.map((e) => e.id)).toEqual(["only"]);
    expect(app.value.envelopesLoadingMore).toBe(false);
    resolvePage([env("page2")]);
    await flush();
    expect(app.value.envelopes.map((e) => e.id)).toEqual(["only"]);
    expect(app.value.selectedMessageIds).toEqual(["only"]);
  });
});
