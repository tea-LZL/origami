import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({
  api: {
    getCachedMessage: vi.fn(),
    getMessage: vi.fn(),
    prefetchDisplay: vi.fn().mockResolvedValue(undefined),
    storeFlags: vi.fn().mockResolvedValue(undefined),
  },
  onSyncEvent: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "./api";
import { app, prefetcher, selectEnvelope } from "./stores.svelte";
import type { Envelope } from "./types";

const cached = vi.mocked(api.getCachedMessage);
const prefetch = vi.mocked(api.prefetchDisplay);

function envelope(id: string, serverUid: number): Envelope {
  return {
    id,
    mailboxId: "folder-1",
    subject: `Subject ${id}`,
    from: [{ name: null, addr: "a@example.org" }],
    to: [],
    date: null,
    receivedAt: null,
    flags: ["Seen"],
    keywords: [],
    hasAttachment: false,
    size: 10,
    serverUid,
    messageId: `<${id}@example.org>`,
    threadId: `<${id}@example.org>`,
    sources: [{ mailboxId: "folder-1", serverUid }],
  } as unknown as Envelope;
}

describe("prefetch cancellation on selection change", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    prefetch.mockClear();
    prefetch.mockResolvedValue(undefined);
    cached.mockReset();
  });

  it("cancel_on_selection_change", async () => {
    const rowA = envelope("a", 1);
    const rowB = envelope("b", 2);

    cached.mockImplementation(async () => ({
      envelope: rowB,
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
    }) as never);

    prefetcher.hover(rowA);
    await selectEnvelope(rowB);
    await vi.advanceTimersByTimeAsync(150);

    expect(prefetch).toHaveBeenCalledTimes(1);
    const requested = prefetch.mock.calls[0][0] as { serverUid: number }[];
    expect(requested.map((r) => r.serverUid)).toEqual([2]);
    expect(requested.every((r) => r.serverUid !== 1)).toBe(true);
  });

  it("probes_every_physical_source_in_parallel", async () => {
    const row = envelope("multi", 1);
    row.sources = [
      { mailboxId: "folder-1", serverUid: 1 },
      { mailboxId: "folder-2", serverUid: 2 },
    ];
    const resolvers: Array<(value: unknown) => void> = [];
    cached.mockImplementation(() => new Promise((resolve) => {
      resolvers.push(resolve);
    }) as never);

    const opening = selectEnvelope(row);
    await Promise.resolve();

    expect(cached).toHaveBeenCalledTimes(2);

    const body = {
      envelope: { ...row, mailboxId: "folder-1", serverUid: 1 },
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
    resolvers[0](null);
    resolvers[1](body);
    await opening;

    expect(app.value.message?.text).toBe("body");
  });
});
