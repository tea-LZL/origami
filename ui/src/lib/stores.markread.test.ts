import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    getCachedMessage: vi.fn(),
    getMessage: vi.fn(),
    storeFlags: vi.fn(),
    listFolders: vi.fn().mockResolvedValue([]),
    searchPage: vi.fn().mockResolvedValue([]),
    searchCount: vi.fn().mockResolvedValue(0),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import { app, selectEnvelope, setPreferences } from "./stores.svelte";
import type { MessageDto } from "./api";
import type { Envelope } from "./types";

const cached = vi.mocked(api.getCachedMessage);
const storeFlags = vi.mocked(api.storeFlags);

// jsdom may not provide localStorage in every environment (see
// MessageView.test.ts for the same pattern).
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

function envelope(id: string, flags: string[] = []): Envelope {
  return {
    id,
    mailboxId: "folder-1",
    subject: id,
    from: [],
    to: [],
    date: null,
    receivedAt: null,
    flags,
    keywords: [],
    hasAttachment: false,
    size: 10,
    serverUid: 1,
    messageId: `<${id}@example.org>`,
    threadId: `<${id}@example.org>`,
    sources: [{ mailboxId: "folder-1", serverUid: 1 }],
  } as unknown as Envelope;
}

function message(id: string, flags: string[] = []): MessageDto {
  return {
    envelope: envelope(id, flags),
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
  } as unknown as MessageDto;
}

describe("delayed mark as read", () => {
  beforeEach(() => {
    stubLocalStorage();
    vi.useFakeTimers();
    vi.clearAllMocks();
    app.value.accounts = [
      { id: "account-1", dbId: "db-1", name: "Work", email: "me@example.org", hasImap: true, hasSmtp: true },
    ] as never;
    app.value.selectedFolderId = "folder-1";
    app.value.envelopes = [envelope("row-1")];
    app.value.markReadDelay = 0;
    app.value.unreadOnly = false;
    cached.mockReset().mockImplementation(async (folderId: string, serverUid: number) =>
      message("row-1"),
    );
    storeFlags.mockReset().mockResolvedValue(undefined);
  });

  it("instant_marks_seen_immediately", async () => {
    app.value.markReadDelay = 0;
    await selectEnvelope(envelope("row-1"));

    expect(storeFlags).toHaveBeenCalledTimes(1);
    expect(app.value.envelopes[0].flags).toContain("Seen");
  });

  it("delayed_waits_before_marking", async () => {
    app.value.markReadDelay = 3;
    const openPromise = selectEnvelope(envelope("row-1"));
    await vi.advanceTimersByTimeAsync(0);
    await openPromise;

    expect(app.value.messageLoading).toBe(false);
    expect(storeFlags).not.toHaveBeenCalled();
    expect(app.value.envelopes[0].flags).not.toContain("Seen");

    await vi.advanceTimersByTimeAsync(3_000);

    expect(storeFlags).toHaveBeenCalledTimes(1);
    expect(app.value.envelopes[0].flags).toContain("Seen");
  });

  it("delayed_cancelled_when_selection_moves", async () => {
    app.value.markReadDelay = 3;
    const first = selectEnvelope(envelope("row-1"));
    await vi.advanceTimersByTimeAsync(0);
    await first;

    // Moving to another row cancels the pending flip for row-1; row-2's own
    // delayed mark is legitimate.
    await selectEnvelope(envelope("row-2"));
    await vi.advanceTimersByTimeAsync(10_000);

    expect(app.value.envelopes.find((item) => item.id === "row-1")!.flags).not.toContain("Seen");
  });

  it("never_never_marks", async () => {
    app.value.markReadDelay = -1;
    await selectEnvelope(envelope("row-1"));
    await vi.advanceTimersByTimeAsync(60_000);

    expect(storeFlags).not.toHaveBeenCalled();
    expect(app.value.envelopes[0].flags).not.toContain("Seen");
  });
});

describe("message zoom", () => {
  beforeEach(() => {
    stubLocalStorage();
    vi.clearAllMocks();
    app.value.messageZoom = 100;
  });

  it("zoom_clamps_to_bounds", () => {
    setPreferences({ messageZoom: 999 });
    expect(app.value.messageZoom).toBe(300);
    setPreferences({ messageZoom: 1 });
    expect(app.value.messageZoom).toBe(50);
  });

  it("zoom_steps", () => {
    setPreferences({ messageZoom: 100 });
    app.value.messageZoom = Math.min(300, app.value.messageZoom + 10);
    expect(app.value.messageZoom).toBe(110);
  });
});
