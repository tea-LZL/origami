import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    listAccounts: vi.fn().mockResolvedValue([]),
    listFolders: vi.fn().mockResolvedValue([]),
    listSavedSearches: vi.fn().mockResolvedValue([]),
    listCorrespondents: vi.fn().mockResolvedValue([]),
    listKeywords: vi.fn().mockResolvedValue([]),
    accountStatuses: vi.fn().mockResolvedValue({}),
    listEnvelopes: vi.fn().mockResolvedValue([]),
    listUnifiedInbox: vi.fn().mockResolvedValue([]),
    listFoldersForAccount: vi.fn(),
    searchPage: vi.fn().mockResolvedValue([]),
    searchCount: vi.fn().mockResolvedValue(0),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import { app, bootstrap } from "./stores.svelte";
import type { Mailbox } from "./types";

const listFolders = vi.mocked(api.listFolders);
const listEnvelopes = vi.mocked(api.listEnvelopes);

function mailbox(id: string, role: Mailbox["role"]): Mailbox {
  return {
    subscribed: true,
    id,
    accountId: "db-1",
    name: id,
    role,
    total: 0,
    unread: 0,
    sourceIds: [id],
  };
}

describe("startup folder restoration", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    app.value.ready = false;
    app.value.selectedFolderId = null;
    listFolders.mockReset().mockResolvedValue([
      mailbox("inbox-1", "Inbox"),
      mailbox("folder-2", "Other"),
    ]);
    listEnvelopes.mockReset().mockResolvedValue([]);
  });

  it("restores_the_last_viewed_folder", async () => {
    localStorage.setItem("origami-last-folder", "folder-2");

    await bootstrap();

    expect(app.value.selectedFolderId).toBe("folder-2");
    expect(listEnvelopes).toHaveBeenCalledWith("folder-2", 1, 200, false);
  });

  it("falls_back_to_first_inbox_without_a_saved_folder", async () => {
    await bootstrap();

    expect(app.value.selectedFolderId).toBe("inbox-1");
  });

  it("restores_unified_inbox", async () => {
    localStorage.setItem("origami-last-folder", "unified");

    await bootstrap();

    expect(app.value.unifiedInbox).toBe(true);
    expect(app.value.selectedFolderId).toBeNull();
  });

  it("falls_back_when_saved_folder_no_longer_exists", async () => {
    localStorage.setItem("origami-last-folder", "deleted-folder");

    await bootstrap();

    expect(app.value.selectedFolderId).toBe("inbox-1");
  });
});
