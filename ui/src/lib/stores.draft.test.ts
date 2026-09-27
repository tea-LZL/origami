import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    loadComposerDraft: vi.fn(),
    deleteComposerDraft: vi.fn(),
    saveComposerDraft: vi.fn(),
    syncComposerDraft: vi.fn(),
    listFolders: vi.fn().mockResolvedValue([]),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import {
  app,
  closeComposer,
  discardComposerDraft,
  openComposer,
} from "./stores.svelte";
import type { AccountDto } from "./api";

function seedAccount() {
  app.value.accounts = [
    { id: "account-1", dbId: "db-1", name: "Work", email: "me@example.org", hasImap: true, hasSmtp: true },
  ] as AccountDto[];
}

function blankDraft() {
  return { to: "", cc: "", bcc: "", subject: "", html: "<p></p>" };
}

const loadDraft = vi.mocked(api.loadComposerDraft);
const deleteDraft = vi.mocked(api.deleteComposerDraft);
const saveDraft = vi.mocked(api.saveComposerDraft);
const syncDraft = vi.mocked(api.syncComposerDraft);

const recovered = {
  accountId: "account-1",
  draft: { to: "a@example.org", cc: "", bcc: "", subject: "old", html: "<p>old</p>" },
  attachments: [],
  threading: { inReplyTo: null, references: [] },
};

describe("composer draft discard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    seedAccount();
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
    loadDraft.mockReset().mockResolvedValue(null);
    deleteDraft.mockReset().mockResolvedValue(undefined);
    saveDraft.mockReset().mockResolvedValue(undefined);
    syncDraft.mockReset().mockResolvedValue(undefined);
    localStorage.clear();
  });

  it("open_composer_flags_recovery", async () => {
    loadDraft.mockResolvedValue(recovered);

    await openComposer();

    expect(app.value.composerRecovered).toBe(true);
    expect(app.value.composerDraft.subject).toBe("old");
    expect(app.value.composerDiscarded).toBe(false);
  });

  it("explicit_draft_is_not_recovery", async () => {
    loadDraft.mockResolvedValue(recovered);

    await openComposer({ subject: "reply", html: "<p>reply</p>" });

    expect(app.value.composerRecovered).toBe(false);
    expect(app.value.composerDraft.subject).toBe("reply");
  });

  it("discard_deletes_everywhere_and_blanks", async () => {
    loadDraft.mockResolvedValue(recovered);
    await openComposer();
    localStorage.setItem("origami-composer-draft", "{\"subject\":\"legacy\"}");

    await discardComposerDraft();

    expect(deleteDraft).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem("origami-composer-draft")).toBeNull();
    expect(app.value.composerDraft).toEqual(blankDraft());
    expect(app.value.composerRecovered).toBe(false);
    expect(app.value.composerDiscarded).toBe(true);
  });

  it("close_after_discard_does_not_resave", async () => {
    loadDraft.mockResolvedValue(recovered);
    await openComposer();
    await discardComposerDraft();

    await closeComposer();

    expect(saveDraft).not.toHaveBeenCalled();
    expect(syncDraft).not.toHaveBeenCalled();
    expect(app.value.composerOpen).toBe(false);
    expect(app.value.composerDiscarded).toBe(false);
  });

  it("close_without_discard_still_saves", async () => {
    loadDraft.mockResolvedValue(recovered);
    await openComposer();
    await discardComposerDraft();
    // Start a fresh message afterwards.
    await openComposer();

    await closeComposer();

    expect(saveDraft).toHaveBeenCalledTimes(1);
  });
});
