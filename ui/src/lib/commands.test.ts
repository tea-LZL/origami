import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    getCachedMessage: vi.fn(),
    getMessage: vi.fn(),
    storeFlags: vi.fn(),
    storeFlagsBatch: vi.fn(),
    listFolders: vi.fn().mockResolvedValue([]),
    searchPage: vi.fn().mockResolvedValue([]),
    searchCount: vi.fn().mockResolvedValue(0),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
    moveMessages: vi.fn(),
    deleteMessages: vi.fn(),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import type { Envelope } from "./types";
import type { MessageDto } from "./api";
import { app } from "./stores.svelte";
import * as stores from "./stores.svelte";
import { commandChordLabel, dispatchCommand, paletteItems } from "./commands";

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

function envelope(id: string, flags: Envelope["flags"] = []): Envelope {
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
    serverUid: 1,
    messageId: `<${id}@example.org>`,
    threadId: `<${id}@example.org>`,
    sources: [{ mailboxId: "inbox", serverUid: 1 }],
  };
}

function loadedMessage(id: string): MessageDto {
  return {
    envelope: envelope(id, ["Seen"]),
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

function keyEvent(init: KeyboardEventInit, target: Element = document.body): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  Object.defineProperty(event, "target", { value: target });
  return event;
}

function resetMail() {
  app.value.selectedMessageIds = ["m1"];
  app.value.selectedEnvelope = envelope("m1");
  app.value.envelopes = [envelope("m1")];
  app.value.selectedFolderId = "inbox";
  app.value.folders = [{
    id: "inbox",
    accountId: "db-1",
    name: "INBOX",
    role: "Inbox",
    subscribed: true,
    total: 1,
    unread: 1,
    sourceIds: [],
  }];
  app.value.searchQuery = "";
  app.value.message = null;
  app.value.activeComposerId = null;
  app.value.preferencesOpen = false;
  app.value.outboxAccountId = null;
  app.value.commandPaletteOpen = false;
  app.value.shortcutsOpen = false;
  app.value.undoAction = null;
}

describe("dispatchCommand", () => {
  beforeEach(() => {
    stubLocalStorage();
    resetMail();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("archives_with_e", () => {
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "e" }))).toBe(true);
    expect(move).toHaveBeenCalledWith("Archive");
  });

  it("stars_mixed_selection_and_unstars_when_all_flagged", () => {
    const flag = vi.spyOn(stores, "setSelectedFlag").mockResolvedValue(undefined);
    app.value.envelopes = [envelope("m1", ["Flagged"]), envelope("m2")];
    app.value.selectedMessageIds = ["m1", "m2"];
    expect(dispatchCommand(keyEvent({ key: "s" }))).toBe(true);
    expect(flag).toHaveBeenCalledWith("Flagged", true);

    flag.mockClear();
    app.value.envelopes = [envelope("m1", ["Flagged"]), envelope("m2", ["Flagged"])];
    expect(dispatchCommand(keyEvent({ key: "s" }))).toBe(true);
    expect(flag).toHaveBeenCalledWith("Flagged", false);
  });

  it("trashes_with_hash_backspace_and_delete_in_inbox", () => {
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "#" }))).toBe(true);
    expect(dispatchCommand(keyEvent({ key: "Backspace" }))).toBe(true);
    expect(dispatchCommand(keyEvent({ key: "Delete" }))).toBe(true);
    expect(move).toHaveBeenCalledTimes(3);
    expect(move).toHaveBeenCalledWith("Trash");
  });

  it("deletes_permanently_in_trash_and_trashes_when_searching", () => {
    const del = vi.spyOn(stores, "deleteSelectedPermanently").mockResolvedValue(undefined);
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    app.value.selectedFolderId = "trash";
    app.value.folders = [{
      id: "trash",
      accountId: "db-1",
      name: "Trash",
      role: "Trash",
      subscribed: true,
      total: 1,
      unread: 0,
      sourceIds: [],
    }];
    expect(dispatchCommand(keyEvent({ key: "Backspace" }))).toBe(true);
    expect(del).toHaveBeenCalledTimes(1);
    expect(move).not.toHaveBeenCalled();

    app.value.searchQuery = "foo";
    expect(dispatchCommand(keyEvent({ key: "Backspace" }))).toBe(true);
    expect(move).toHaveBeenCalledWith("Trash");
  });

  it("junks_with_exclamation", () => {
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "!" }))).toBe(true);
    expect(move).toHaveBeenCalledWith("Junk");
  });

  it("reply_keys_require_loaded_matching_message", () => {
    const reply = vi.spyOn(stores, "openReplyComposer");
    const forward = vi.spyOn(stores, "forwardMessage").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "r" }))).toBe(false);
    expect(dispatchCommand(keyEvent({ key: "a" }))).toBe(false);
    expect(dispatchCommand(keyEvent({ key: "f" }))).toBe(false);
    expect(reply).not.toHaveBeenCalled();
    expect(forward).not.toHaveBeenCalled();

    app.value.message = loadedMessage("m1");
    expect(dispatchCommand(keyEvent({ key: "r" }))).toBe(true);
    expect(reply).toHaveBeenCalledWith("reply");
    expect(dispatchCommand(keyEvent({ key: "a" }))).toBe(true);
    expect(reply).toHaveBeenCalledWith("replyAll");
    expect(dispatchCommand(keyEvent({ key: "f" }))).toBe(true);
    expect(forward).toHaveBeenCalled();
  });

  it("marks_unread_and_read_with_shift_u_and_i", () => {
    const flag = vi.spyOn(stores, "setSelectedFlag").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "U", shiftKey: true }))).toBe(true);
    expect(flag).toHaveBeenCalledWith("Seen", false);
    expect(dispatchCommand(keyEvent({ key: "I", shiftKey: true }))).toBe(true);
    expect(flag).toHaveBeenCalledWith("Seen", true);
  });

  it("undoes_only_when_an_action_is_live", () => {
    const undo = vi.spyOn(stores, "undoLastMessageAction");
    expect(dispatchCommand(keyEvent({ key: "z" }))).toBe(false);
    expect(undo).not.toHaveBeenCalled();
    app.value.undoAction = { message: "Archived", token: 1 };
    expect(dispatchCommand(keyEvent({ key: "z" }))).toBe(true);
    expect(undo).toHaveBeenCalledTimes(1);
  });

  it("opens_composer_with_n", () => {
    const compose = vi.spyOn(stores, "openComposer");
    expect(dispatchCommand(keyEvent({ key: "n" }))).toBe(true);
    expect(compose).toHaveBeenCalledTimes(1);
  });

  it("opens_palette_with_ctrl_k_or_meta_k_but_not_both", () => {
    expect(dispatchCommand(keyEvent({ key: "k", ctrlKey: true }))).toBe(true);
    expect(app.value.commandPaletteOpen).toBe(true);
    app.value.commandPaletteOpen = false;
    expect(dispatchCommand(keyEvent({ key: "k", metaKey: true }))).toBe(true);
    expect(app.value.commandPaletteOpen).toBe(true);
    app.value.commandPaletteOpen = false;
    expect(dispatchCommand(keyEvent({ key: "k", ctrlKey: true, metaKey: true }))).toBe(false);
    expect(app.value.commandPaletteOpen).toBe(false);
  });

  it("opens_shortcuts_with_question_mark", () => {
    expect(dispatchCommand(keyEvent({ key: "?" }))).toBe(true);
    expect(app.value.shortcutsOpen).toBe(true);
    expect(app.value.commandPaletteOpen).toBe(false);
  });

  it("ignores_key_repeat", () => {
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "e", repeat: true }))).toBe(false);
    expect(move).not.toHaveBeenCalled();
  });

  it("does_not_treat_ctrl_a_as_reply_all", () => {
    app.value.message = loadedMessage("m1");
    const reply = vi.spyOn(stores, "openReplyComposer");
    expect(dispatchCommand(keyEvent({ key: "a", ctrlKey: true }))).toBe(false);
    expect(reply).not.toHaveBeenCalled();
  });

  it("skips_letter_keys_in_inputs_but_allows_ctrl_k", () => {
    const input = document.createElement("input");
    document.body.append(input);
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "e" }, input))).toBe(false);
    expect(move).not.toHaveBeenCalled();
    expect(dispatchCommand(keyEvent({ key: "k", ctrlKey: true }, input))).toBe(true);
    expect(app.value.commandPaletteOpen).toBe(true);
    input.remove();
  });

  it("skips_keys_when_composer_is_active", () => {
    app.value.activeComposerId = "draft-1";
    const move = vi.spyOn(stores, "moveSelectedToRole").mockResolvedValue(undefined);
    expect(dispatchCommand(keyEvent({ key: "e" }))).toBe(false);
    expect(move).not.toHaveBeenCalled();
    expect(dispatchCommand(keyEvent({ key: "k", ctrlKey: true }))).toBe(false);
    expect(app.value.commandPaletteOpen).toBe(false);
  });

  it("skips_ctrl_k_when_preferences_are_open", () => {
    app.value.preferencesOpen = true;
    expect(dispatchCommand(keyEvent({ key: "k", ctrlKey: true }))).toBe(false);
    expect(app.value.commandPaletteOpen).toBe(false);
  });
});

describe("commandChordLabel", () => {
  it("formats_archive_and_trash_chords", () => {
    expect(commandChordLabel("archive")).toBe("E");
    expect(commandChordLabel("trash")).toBe("⌫");
  });
});

describe("paletteItems", () => {
  function mailbox(
    id: string,
    name: string,
    role: Envelope extends never ? never : import("./types").Mailbox["role"],
    accountId: string,
  ): import("./types").Mailbox {
    return {
      id,
      name,
      role,
      accountId,
      subscribed: true,
      total: 1,
      unread: 0,
      sourceIds: [],
    };
  }

  beforeEach(() => {
    stubLocalStorage();
    resetMail();
    app.value.accounts = [
      { id: "acc-1", dbId: "db-1", name: "Work", email: "w@example.org", hasImap: true, hasSmtp: true },
      { id: "acc-2", dbId: "db-2", name: "Home", email: "h@example.org", hasImap: true, hasSmtp: true },
    ];
    app.value.folders = [
      mailbox("inbox", "INBOX", "Inbox", "db-1"),
      mailbox("archive", "Archive", "Archive", "db-1"),
      mailbox("all", "[Gmail]/All Mail", "Other", "db-1"),
      mailbox("home-inbox", "INBOX", "Inbox", "db-2"),
    ];
    app.value.envelopes = [envelope("m1")];
    app.value.selectedMessageIds = ["m1"];
    app.value.selectedFolderId = "inbox";
  });

  it("empty_query_is_cheat_sheet_without_folder_rows_or_open_palette", () => {
    app.value.selectedMessageIds = [];
    const items = paletteItems("");
    const titles = items.map((item) => item.title);
    expect(titles).toContain("Archive");
    expect(titles).toContain("Trash");
    expect(titles).toContain("Compose new message");
    expect(titles).toContain("Go to Unified Inbox");
    expect(titles.some((title) => title.includes("Open command palette"))).toBe(false);
    expect(titles.some((title) => title.startsWith("Go to Work"))).toBe(false);
    expect(titles.some((title) => title.startsWith("Move to"))).toBe(false);
    expect(items.find((item) => item.id === "archive")?.enabled).toBe(false);
  });

  it("typed_query_adds_go_to_and_same_account_move_to_with_sidebar_labels", () => {
    const inboxTitles = paletteItems("inb").map((item) => item.title);
    expect(inboxTitles).toContain("Go to Work / Inbox");
    expect(inboxTitles).not.toContain("Move to Work / Inbox");
    expect(inboxTitles).not.toContain("Move to Home / Inbox");
    expect(inboxTitles.some((title) => /all mail/i.test(title))).toBe(false);

    const archiveItems = paletteItems("arc");
    const archiveTitles = archiveItems.map((item) => item.title);
    expect(archiveTitles).toContain("Move to Work / Archive");
    expect(archiveTitles).not.toContain("Move to Home / Inbox");
    expect(archiveItems.every((item) => item.enabled)).toBe(true);
  });

  it("omits_move_to_for_mixed_account_selection", () => {
    app.value.envelopes = [
      { ...envelope("m1"), mailboxId: "inbox", sources: [{ mailboxId: "inbox", serverUid: 1 }] },
      { ...envelope("m2"), mailboxId: "home-inbox", sources: [{ mailboxId: "home-inbox", serverUid: 2 }] },
    ];
    app.value.selectedMessageIds = ["m1", "m2"];
    const titles = paletteItems("inb").map((item) => item.title);
    expect(titles.every((title) => !title.startsWith("Move to"))).toBe(true);
  });

  it("returns_empty_when_nothing_matches", () => {
    expect(paletteItems("zzzz")).toEqual([]);
  });

  it("caps_results_at_40", () => {
    app.value.folders = Array.from({ length: 50 }, (_, index) =>
      mailbox(`f${index}`, `Label${index}`, "Other", "db-1"),
    );
    expect(paletteItems("label").length).toBeLessThanOrEqual(40);
  });

  it("ranks_word_start_archive_above_mid_word_match", () => {
    app.value.folders = [
      mailbox("research", "Research archive", "Other", "db-1"),
    ];
    const titles = paletteItems("arc").map((item) => item.title);
    expect(titles.indexOf("Archive")).toBeGreaterThanOrEqual(0);
    expect(titles.indexOf("Archive")).toBeLessThan(titles.indexOf("Go to Work / Research archive"));
  });
});
