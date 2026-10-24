import { describe, expect, test } from "vitest";
import type { Envelope, Flag } from "./types";
import { insertIncomingEnvelope, type IncomingView } from "./incoming";

let nextUid = 1;

function envelope(id: string, receivedAt: number | null, flags: Flag[] = []): Envelope {
  return {
    id,
    mailboxId: "inbox",
    subject: `subject ${id}`,
    from: [],
    to: [],
    date: null,
    receivedAt,
    flags,
    hasAttachment: false,
    size: 10,
    serverUid: nextUid++,
    messageId: null,
    threadId: null,
    keywords: [],
    sources: [],
  };
}

function view(overrides: Partial<IncomingView> = {}): IncomingView {
  return {
    envelopes: [envelope("b", 200), envelope("a", 100)],
    unreadOnly: false,
    searchActive: false,
    page: 1,
    unifiedInbox: false,
    relevant: true,
    ...overrides,
  };
}

describe("insertIncomingEnvelope", () => {
  test("inserts a new message at the top of page one", () => {
    const result = insertIncomingEnvelope(view(), envelope("c", 300));
    expect(result.inserted).toBe(true);
    expect(result.envelopes.map((item) => item.id)).toEqual(["c", "b", "a"]);
  });

  test("keeps received-time order instead of assuming arrival order", () => {
    const result = insertIncomingEnvelope(view(), envelope("middle", 150));
    expect(result.envelopes.map((item) => item.id)).toEqual(["b", "middle", "a"]);
  });

  test("newer row wins a received-time tie", () => {
    const result = insertIncomingEnvelope(view(), envelope("tie", 200));
    expect(result.envelopes.map((item) => item.id)).toEqual(["tie", "b", "a"]);
  });

  test("ignores a logical message already in the page", () => {
    const result = insertIncomingEnvelope(view(), envelope("b", 999));
    expect(result.inserted).toBe(false);
    expect(result.envelopes.map((item) => item.id)).toEqual(["b", "a"]);
  });

  test("drops a seen message while the unread-only filter is on", () => {
    const result = insertIncomingEnvelope(
      view({ unreadOnly: true }),
      envelope("c", 300, ["Seen"]),
    );
    expect(result.inserted).toBe(false);
    expect(result.envelopes.map((item) => item.id)).toEqual(["b", "a"]);
  });

  test("drops an unread message when the view is a search or a later page", () => {
    for (const overrides of [{ searchActive: true }, { page: 2 }]) {
      const result = insertIncomingEnvelope(view(overrides), envelope("c", 300));
      expect(result.inserted, JSON.stringify(overrides)).toBe(false);
      expect(result.envelopes).toHaveLength(2);
    }
  });

  test("drops a message from a folder outside the current view", () => {
    const result = insertIncomingEnvelope(view({ relevant: false }), envelope("c", 300));
    expect(result.inserted).toBe(false);
    expect(result.envelopes.map((item) => item.id)).toEqual(["b", "a"]);
  });

  test("drops a second copy of a message already on the page", () => {
    const visible = {
      ...envelope("existing", 200),
      mailboxId: "inbox",
      serverUid: 7,
      sources: [{ mailboxId: "inbox", serverUid: 7 }],
    };
    const copy = {
      ...envelope("other-row", 200),
      mailboxId: "inbox",
      serverUid: 7,
      sources: [{ mailboxId: "inbox", serverUid: 7 }],
    };
    const result = insertIncomingEnvelope(
      view({ envelopes: [visible] }),
      copy,
    );
    expect(result.inserted).toBe(false);
    expect(result.envelopes).toHaveLength(1);
  });

  test("drops an envelope without a usable row id", () => {
    const result = insertIncomingEnvelope(view(), envelope("", 300));
    expect(result.inserted).toBe(false);
    expect(result.envelopes).toHaveLength(2);
  });

  test("inserts into an empty page", () => {
    const result = insertIncomingEnvelope(view({ envelopes: [] }), envelope("only", 10));
    expect(result.inserted).toBe(true);
    expect(result.envelopes.map((item) => item.id)).toEqual(["only"]);
  });
});
