import { describe, expect, it } from "vitest";
import type { Envelope } from "./types";
import {
  applyUnreadListReload,
  folderViewKey,
  retainForUnreadFilter,
} from "./unreadList";

function envelope(partial: Partial<Envelope> & Pick<Envelope, "id">): Envelope {
  return {
    mailboxId: "inbox",
    subject: partial.id,
    from: [],
    to: [],
    date: null,
    flags: [],
    hasAttachment: false,
    size: 1,
    serverUid: 1,
    messageId: `${partial.id}@example.org`,
    threadId: partial.id,
    keywords: [],
    sources: [{ mailboxId: "inbox", serverUid: 1 }],
    ...partial,
  };
}

describe("retainForUnreadFilter", () => {
  it("keeps the open Seen row in the unread-only list", () => {
    const open = envelope({ id: "open", flags: ["Seen"] });
    const otherRead = envelope({ id: "other-read", flags: ["Seen"] });
    const unread = envelope({ id: "unread" });

    expect(
      retainForUnreadFilter([open, otherRead, unread], true, ["open"]).map((item) => item.id),
    ).toEqual(["open", "unread"]);
  });

  it("does not filter when unread-only is off", () => {
    const open = envelope({ id: "open", flags: ["Seen"] });
    const unread = envelope({ id: "unread" });
    expect(
      retainForUnreadFilter([open, unread], false, ["open"]).map((item) => item.id),
    ).toEqual(["open", "unread"]);
  });
});

describe("applyUnreadListReload", () => {
  it("unions the selected Seen row back after an unread-only folder refresh", () => {
    const unread = envelope({ id: "unread" });
    const open = envelope({ id: "open", flags: ["Seen"], subject: "Still open" });
    const gone = envelope({ id: "gone", flags: ["Seen"] });
    const fresh = envelope({ id: "fresh" });

    const result = applyUnreadListReload({
      loaded: [fresh, unread],
      previous: [unread, open, gone],
      selectedEnvelope: open,
      selectedMessageIds: ["open"],
      unreadOnly: true,
    });

    expect(result.envelopes.map((item) => item.id)).toEqual(["fresh", "open", "unread"]);
    expect(result.envelopes.find((item) => item.id === "open")?.flags).toContain("Seen");
    expect(result.selectedMessageIds).toEqual(["open"]);
  });

  it("does not drop selectedMessageIds just because the store omitted them", () => {
    const unread = envelope({ id: "unread" });
    const open = envelope({ id: "open", flags: ["Seen"] });
    const checked = envelope({ id: "checked", flags: ["Seen"] });

    const result = applyUnreadListReload({
      loaded: [unread],
      previous: [unread, open, checked],
      selectedEnvelope: open,
      selectedMessageIds: ["open", "checked", "missing-id"],
      unreadOnly: true,
    });

    expect(result.envelopes.map((item) => item.id)).toEqual(["unread", "open", "checked"]);
    expect(result.selectedMessageIds).toEqual(["open", "checked", "missing-id"]);
  });

  it("does not mix kept rows into an all-mail reload", () => {
    const unread = envelope({ id: "unread" });
    const open = envelope({ id: "open", flags: ["Seen"] });

    const result = applyUnreadListReload({
      loaded: [unread],
      previous: [unread, open],
      selectedEnvelope: open,
      selectedMessageIds: ["open"],
      unreadOnly: false,
    });

    expect(result.envelopes.map((item) => item.id)).toEqual(["unread"]);
    expect(result.selectedMessageIds).toEqual([]);
  });
});

describe("folderViewKey", () => {
  it("keeps unread and all caches on distinct keys", () => {
    expect(folderViewKey("inbox", true)).toBe("inbox:unread");
    expect(folderViewKey("inbox", false)).toBe("inbox:all");
    expect(folderViewKey("inbox", true)).not.toBe(folderViewKey("inbox", false));
    expect(folderViewKey("sent", true)).not.toBe(folderViewKey("inbox", true));
  });
});
