import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    listComposerDrafts: vi.fn(),
    saveComposerDraft: vi.fn(),
    syncComposerDraft: vi.fn(),
    deleteComposerDraft: vi.fn(),
    sendMessage: vi.fn(),
    listFolders: vi.fn().mockResolvedValue([]),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api, type AccountDto, type SavedComposerDraft } from "./api";
import {
  app,
  discardComposer,
  flushComposerSave,
  minimizeComposer,
  openComposer,
  requestDiscardComposer,
  restoreComposerSessions,
  scheduleComposerSave,
  sendComposer,
  syncComposerNow,
} from "./stores.svelte";

function seedAccount() {
  app.value.accounts = [
    {
      id: "account-1",
      dbId: "db-1",
      name: "Work",
      email: "me@example.org",
      hasImap: true,
      hasSmtp: true,
    },
  ] as AccountDto[];
}

const saved = (fields: Partial<SavedComposerDraft["draft"]>): SavedComposerDraft => ({
  accountId: "account-1",
  draft: {
    to: "",
    cc: "",
    bcc: "",
    subject: "",
    html: "<p></p>",
    composeMode: "rich",
    ...fields,
  },
  attachments: [],
  threading: { inReplyTo: null, references: [] },
});

const listDrafts = vi.mocked(api.listComposerDrafts);
const saveDraft = vi.mocked(api.saveComposerDraft);
const syncDraft = vi.mocked(api.syncComposerDraft);
const deleteDraft = vi.mocked(api.deleteComposerDraft);
const sendMessage = vi.mocked(api.sendMessage);

describe("composer sessions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
    Object.assign(app.value, {
      accounts: [],
      folders: [],
      selectedFolderId: null,
      composerSessions: [],
      activeComposerId: null,
      sendingComposerId: null,
      discardConfirmId: null,
      lastError: null,
      lastNotice: null,
    });
    listDrafts.mockReset().mockResolvedValue([]);
    saveDraft.mockReset().mockResolvedValue(undefined);
    syncDraft.mockReset().mockResolvedValue(undefined);
    deleteDraft.mockReset().mockResolvedValue(undefined);
    sendMessage.mockReset().mockResolvedValue({ queued: false });
    localStorage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("restore_skips_without_accounts", async () => {
    app.value.accounts = [];

    await restoreComposerSessions();

    expect(listDrafts).not.toHaveBeenCalled();
    expect(app.value.composerSessions).toEqual([]);
  });

  it("restore_maps_rows_to_minimized_sessions_and_drops_blank", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([
      { id: "b", draft: saved({ subject: "real" }) },
      { id: "a", draft: saved({}) },
    ]);

    await restoreComposerSessions();

    expect(app.value.composerSessions.map((session) => session.id)).toEqual(["b"]);
    expect(app.value.composerSessions[0].draft.subject).toBe("real");
    expect(app.value.activeComposerId).toBeNull();
    expect(app.value.composerSessions[0].saveState).toBe("saved");
    expect(deleteDraft).toHaveBeenCalledWith("a");
  });

  it("legacy_composer_row_is_adopted", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([{ id: "composer", draft: saved({ subject: "old" }) }]);

    await restoreComposerSessions();

    expect(app.value.composerSessions.map((session) => session.id)).toEqual(["composer"]);
    expect(app.value.composerSessions[0].draft.subject).toBe("old");
  });

  it("corrupt_legacy_localstorage_is_dropped", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([]);
    localStorage.setItem("origami-composer-draft", "{not json");

    await restoreComposerSessions();

    expect(app.value.composerSessions).toEqual([]);
    expect(localStorage.getItem("origami-composer-draft")).toBeNull();
  });

  it("legacy_localstorage_draft_becomes_session", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([]);
    localStorage.setItem(
      "origami-composer-draft",
      JSON.stringify({ to: "a@example.org", cc: "", bcc: "", subject: "legacy", html: "<p>x</p>" }),
    );

    await restoreComposerSessions();

    expect(app.value.composerSessions).toHaveLength(1);
    expect(app.value.composerSessions[0].draft.subject).toBe("legacy");
    expect(localStorage.getItem("origami-composer-draft")).toBeNull();
  });

  it("open_composer_creates_a_new_active_session", async () => {
    seedAccount();

    openComposer({ subject: "one" });
    openComposer({ subject: "two" });

    expect(app.value.composerSessions).toHaveLength(2);
    expect(app.value.composerSessions.map((session) => session.draft.subject)).toEqual(["one", "two"]);
    expect(app.value.activeComposerId).toBe(app.value.composerSessions[1].id);
  });

  it("pending_saves_are_per_session", async () => {
    vi.useFakeTimers();
    seedAccount();
    openComposer({ subject: "one" });
    const first = app.value.composerSessions[0].id;
    openComposer({ subject: "two" });
    const second = app.value.composerSessions[1].id;

    scheduleComposerSave(first);
    scheduleComposerSave(second);
    await vi.runAllTimersAsync();

    expect(saveDraft).toHaveBeenCalledWith(
      first,
      expect.objectContaining({ draft: expect.objectContaining({ subject: "one" }) }),
    );
    expect(saveDraft).toHaveBeenCalledWith(
      second,
      expect.objectContaining({ draft: expect.objectContaining({ subject: "two" }) }),
    );
  });

  it("blank_body_counts_as_blank", async () => {
    vi.useFakeTimers();
    seedAccount();
    openComposer({ html: "<p>&nbsp;</p>" });

    scheduleComposerSave(app.value.composerSessions[0].id);
    await vi.runAllTimersAsync();

    expect(saveDraft).not.toHaveBeenCalled();
  });

  it("discard_confirms_then_removes_and_activates_neighbor", async () => {
    seedAccount();
    openComposer({ subject: "a" });
    openComposer({ subject: "b" });
    const [first, second] = app.value.composerSessions.map((session) => session.id);

    requestDiscardComposer(first);
    expect(app.value.discardConfirmId).toBe(first);

    await discardComposer(first);

    expect(deleteDraft).toHaveBeenCalledWith(first);
    expect(app.value.composerSessions.map((session) => session.id)).toEqual([second]);
    expect(app.value.activeComposerId).toBe(second);
    expect(app.value.discardConfirmId).toBeNull();
  });

  it("blank_discard_skips_confirmation", async () => {
    seedAccount();
    openComposer();
    const id = app.value.composerSessions[0].id;

    requestDiscardComposer(id);

    expect(app.value.discardConfirmId).toBeNull();
    expect(app.value.composerSessions).toEqual([]);
    expect(deleteDraft).toHaveBeenCalledWith(id);
  });

  it("discard_during_inflight_save_does_not_resurrect", async () => {
    seedAccount();
    openComposer({ subject: "race" });
    const id = app.value.composerSessions[0].id;
    let release: (() => void) | undefined;
    saveDraft.mockImplementation(() => new Promise<void>((resolve) => {
      release = resolve;
    }));

    const saving = flushComposerSave(id);
    await discardComposer(id);
    expect(deleteDraft).toHaveBeenCalledWith(id);

    release?.();
    await saving;

    expect(deleteDraft).toHaveBeenCalledTimes(2);
    expect(app.value.composerSessions).toEqual([]);
  });

  it("edit_during_sync_causes_followup_sync", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([{ id: "x", draft: saved({ subject: "s" }) }]);
    await restoreComposerSessions();
    let release: (() => void) | undefined;
    syncDraft.mockImplementationOnce(() => new Promise<void>((resolve) => {
      release = resolve;
    }));

    const syncing = syncComposerNow("x");
    await vi.waitFor(() => expect(syncDraft).toHaveBeenCalledTimes(1));
    app.value.composerSessions[0].draft.subject = "s2";
    release?.();
    await syncing;

    await syncComposerNow("x");

    expect(syncDraft).toHaveBeenCalledTimes(2);
  });

  it("send_removes_session_and_draft", async () => {
    seedAccount();
    sendMessage.mockResolvedValue({ queued: false });
    openComposer({ to: "a@example.org" });
    const id = app.value.composerSessions[0].id;

    await sendComposer(id);

    expect(deleteDraft).toHaveBeenCalledWith(id);
    expect(app.value.composerSessions).toEqual([]);
  });

  it("send_blocks_invalid_recipient", async () => {
    seedAccount();
    openComposer({ to: "nope" });

    await sendComposer(app.value.composerSessions[0].id);

    expect(sendMessage).not.toHaveBeenCalled();
    expect(app.value.lastError).toContain("invalid recipient");
    expect(app.value.composerSessions).toHaveLength(1);
  });

  it("minimize_flushes_and_syncs", async () => {
    vi.useFakeTimers();
    seedAccount();
    openComposer({ subject: "keep" });
    const id = app.value.composerSessions[0].id;

    minimizeComposer();
    await vi.runAllTimersAsync();

    expect(app.value.activeComposerId).toBeNull();
    expect(saveDraft).toHaveBeenCalledWith(id, expect.anything());
    expect(syncDraft).toHaveBeenCalledWith(id);
  });

  it("sync_error_then_retry_recovers", async () => {
    seedAccount();
    listDrafts.mockResolvedValue([{ id: "x", draft: saved({ subject: "s" }) }]);
    await restoreComposerSessions();
    syncDraft
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(undefined);

    await syncComposerNow("x");
    expect(app.value.composerSessions[0].syncState).toBe("error");

    await syncComposerNow("x");
    expect(app.value.composerSessions[0].syncState).toBe("synced");
  });
});
