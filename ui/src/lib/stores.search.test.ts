import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    searchPage: vi.fn(),
    searchCount: vi.fn(),
    listEnvelopes: vi.fn(),
    listFolders: vi.fn().mockResolvedValue([]),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import { app, searchMessages } from "./stores.svelte";

const searchPage = vi.mocked(api.searchPage);
const searchCount = vi.mocked(api.searchCount);

function envelope(id: string) {
  return {
    id,
    mailboxId: "folder-1",
    subject: `invoice ${id}`,
    from: [],
    to: [],
    date: null,
    flags: [],
    keywords: [],
    hasAttachment: false,
    size: 1,
    serverUid: 1,
    messageId: `<${id}@example.org>`,
    threadId: `<${id}@example.org>`,
    sources: [{ mailboxId: "folder-1", serverUid: 1 }],
  };
}

describe("search result total", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    app.value.searchQuery = "";
    app.value.searchTotal = null;
    app.value.selectedFolderId = "folder-1";
    searchPage.mockReset().mockResolvedValue([]);
    searchCount.mockReset().mockResolvedValue(0);
  });

  it("search_total_captured_with_page", async () => {
    searchPage.mockResolvedValue([envelope("e1")]);
    searchCount.mockResolvedValue(42);

    await searchMessages("invoice");

    expect(app.value.searchTotal).toBe(42);
    expect(app.value.envelopes).toHaveLength(1);
    // Same unread token goes to both calls.
    const token = searchPage.mock.calls[0][0];
    expect(searchCount.mock.calls[0][0]).toBe(token);
  });

  it("count_failure_leaves_loaded_fallback", async () => {
    searchPage.mockResolvedValue([envelope("e1"), envelope("e2")]);
    searchCount.mockRejectedValue(new Error("boom"));

    await searchMessages("invoice");

    expect(app.value.searchTotal).toBeNull();
  });

  it("clearing_query_resets_total", async () => {
    app.value.searchTotal = 42;
    app.value.unifiedInbox = false;

    await searchMessages("   ");

    expect(app.value.searchTotal).toBeNull();
  });
});
