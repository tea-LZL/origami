import { describe, expect, it } from "vitest";
import type { Envelope } from "./types";
import { mergeThreadMembers, threadKey } from "./threads";

function envelope(id: string): Envelope {
  return {
    id,
    mailboxId: "inbox",
    subject: id,
    from: [],
    to: [],
    date: null,
    receivedAt: null,
    flags: [],
    keywords: [],
    hasAttachment: false,
    size: 1,
    serverUid: 1,
    messageId: `<${id}@example.org>`,
    threadId: "t",
    sources: [],
  } as Envelope;
}

describe("mergeThreadMembers", () => {
  it("appends_cross_folder_rows_without_duplicates", () => {
    const local = [envelope("a"), envelope("b")];
    const cross = [envelope("c"), envelope("a")];

    const merged = mergeThreadMembers(local, cross);

    expect(merged.map((item) => item.id)).toEqual(["a", "b", "c"]);
  });

  it("returns_local_when_no_cross_rows", () => {
    expect(mergeThreadMembers([envelope("a")], [])).toHaveLength(1);
  });
});
