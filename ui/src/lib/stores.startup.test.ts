import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    listAccounts: vi.fn().mockResolvedValue([]),
    listFolders: vi.fn().mockResolvedValue([]),
    listSavedSearches: vi.fn().mockResolvedValue([]),
    listCorrespondents: vi.fn().mockResolvedValue([]),
    listKeywords: vi.fn().mockResolvedValue([]),
    accountStatuses: vi.fn().mockResolvedValue({}),
    listComposerDrafts: vi.fn().mockResolvedValue([]),
    deleteComposerDraft: vi.fn().mockResolvedValue(undefined),
    listEnvelopes: vi.fn().mockResolvedValue([]),
    listUnifiedInbox: vi.fn().mockResolvedValue([]),
    listFoldersForAccount: vi.fn(),
    searchPage: vi.fn().mockResolvedValue([]),
    searchCount: vi.fn().mockResolvedValue(0),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
    prefetchSelectedFolder: vi.fn().mockResolvedValue(undefined),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import { app, bootstrap, openComposer } from "./stores.svelte";
import type { Mailbox } from "./types";

const listFolders = vi.mocked(api.listFolders);
const listEnvelopes = vi.mocked(api.listEnvelopes);
const listAccounts = vi.mocked(api.listAccounts);
const listComposerDrafts = vi.mocked(api.listComposerDrafts);
const listKeywords = vi.mocked(api.listKeywords);
const listCorrespondents = vi.mocked(api.listCorrespondents);
const listSavedSearches = vi.mocked(api.listSavedSearches);

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
    // jsdom may not provide localStorage in every environment (see
    // MessageView.test.ts for the same pattern).
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
    vi.clearAllMocks();
    listKeywords.mockReset().mockResolvedValue([]);
    listCorrespondents.mockReset().mockResolvedValue([]);
    listSavedSearches.mockReset().mockResolvedValue([]);
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

  it("loads_the_folder_before_draft_restore_finishes", async () => {
    listAccounts.mockResolvedValue([{
      id: "account-1",
      dbId: "db-1",
      name: "Work",
      email: "me@example.org",
      hasImap: true,
      hasSmtp: true,
    }]);
    let finishDrafts: (rows: unknown[]) => void = () => {};
    listComposerDrafts.mockReturnValue(new Promise((resolve) => {
      finishDrafts = resolve;
    }) as never);

    const booting = bootstrap();
    await vi.waitFor(() => {
      expect(listEnvelopes).toHaveBeenCalledWith("inbox-1", 1, 200, false);
    });
    expect(app.value.selectedFolderId).toBe("inbox-1");

    finishDrafts([]);
    await booting;
  });

  it("paints_the_first_folder_without_waiting_for_the_catalog_scans", async () => {
    let finishKeywords: (rows: unknown[]) => void = () => {};
    let finishCorrespondents: (rows: unknown[]) => void = () => {};
    let finishSaved: (rows: unknown[]) => void = () => {};
    listKeywords.mockReturnValue(new Promise((resolve) => {
      finishKeywords = resolve;
    }) as never);
    listCorrespondents.mockReturnValue(new Promise((resolve) => {
      finishCorrespondents = resolve;
    }) as never);
    listSavedSearches.mockReturnValue(new Promise((resolve) => {
      finishSaved = resolve;
    }) as never);

    const booting = bootstrap();
    await vi.waitFor(() => {
      expect(listEnvelopes).toHaveBeenCalledWith("inbox-1", 1, 200, false);
    });
    expect(app.value.ready).toBe(true);

    finishKeywords([]);
    finishCorrespondents([]);
    finishSaved([]);
    await booting;
  });

  it("keeps_a_composer_opened_while_drafts_restore", async () => {
    listAccounts.mockResolvedValue([{
      id: "account-1",
      dbId: "db-1",
      name: "Work",
      email: "me@example.org",
      hasImap: true,
      hasSmtp: true,
    }]);
    let finishDrafts: (rows: unknown[]) => void = () => {};
    listComposerDrafts.mockReturnValue(new Promise((resolve) => {
      finishDrafts = resolve;
    }) as never);

    const booting = bootstrap();
    await vi.waitFor(() => {
      expect(listEnvelopes).toHaveBeenCalledWith("inbox-1", 1, 200, false);
    });
    expect(listComposerDrafts).toHaveBeenCalledTimes(1);

    openComposer({ to: "someone@example.org" });
    const openedId = app.value.activeComposerId;
    expect(openedId).not.toBeNull();

    finishDrafts([{
      id: "restored-draft",
      draft: {
        accountId: "account-1",
        draft: {
          to: "restored@example.org",
          cc: "",
          bcc: "",
          subject: "Restored draft",
          html: "<p>restored</p>",
          composeMode: "rich",
        },
        attachments: [],
        threading: { inReplyTo: null, references: [] },
      },
    }]);
    await booting;

    expect(app.value.composerSessions.some((session) => session.id === openedId)).toBe(true);
    expect(app.value.composerSessions.some((session) => session.id === "restored-draft")).toBe(true);
    expect(app.value.activeComposerId).toBe(openedId);
  });
});
