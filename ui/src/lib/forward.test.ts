import { describe, expect, it, vi } from "vitest";

import type { AttachmentMeta, MessageDto } from "./api";
import { INLINE_IMAGE_MAX_BYTES } from "./inlineImage";
import { buildForward } from "./forward";

const inlineImage: AttachmentMeta = {
  index: 0,
  partPath: "1.1",
  name: "logo.png",
  mime: "image/png",
  size: 10,
  inline: true,
  cid: "logo@example.com",
};

const fileAttachment: AttachmentMeta = {
  index: 1,
  partPath: "2",
  name: "report.pdf",
  mime: "application/pdf",
  size: 1024,
  inline: false,
  cid: null,
};

function message(overrides: Partial<MessageDto> = {}): MessageDto {
  return {
    envelope: {
      id: "m1",
      mailboxId: "folder-1",
      subject: "Original subject",
      from: [{ name: "Alice", addr: "alice@example.org" }],
      to: [{ name: null, addr: "me@example.org" }],
      date: "Fri, 18 Sep 2026 09:52:41 +0000",
      flags: ["Seen"],
      hasAttachment: true,
      size: 2048,
      serverUid: 5,
      messageId: "<original@example.org>",
      threadId: null,
      keywords: [],
      sources: [{ mailboxId: "folder-1", serverUid: 5 }],
    },
    text: "Hello",
    html: '<p>Hello <img src="cid:logo@example.com"></p>',
    headers: {
      cc: [{ name: null, addr: "cc@example.org" }],
      replyTo: [],
      sender: [],
      messageId: "<original@example.org>",
      inReplyTo: [],
      references: [],
      listUnsubscribe: [],
      authenticationResults: null,
    },
    attachments: [inlineImage, fileAttachment],
    parts: [],
    parseWarnings: [],
    ...overrides,
  };
}

function fetcher(map: Record<number, string>) {
  return vi.fn(async (index: number) => {
    const value = map[index];
    if (value === undefined) throw new Error(`no attachment ${index}`);
    return value;
  });
}

describe("buildForward", () => {
  it("builds a forwarded header block, sanitized body, inlined image, and file attachment", async () => {
    const result = await buildForward(message(), fetcher({ 0: "aW1n", 1: "cGRm" }));

    expect(result.html).toContain("---------- Forwarded message ----------");
    expect(result.html).toContain("<b>From:</b> Alice &lt;alice@example.org&gt;");
    expect(result.html).toContain("<b>Subject:</b> Original subject");
    expect(result.html).toContain("<b>To:</b> me@example.org");
    expect(result.html).toContain("<b>Cc:</b> cc@example.org");
    expect(result.html).toContain('src="data:image/png;base64,aW1n"');
    expect(result.attachments).toEqual([
      { name: "report.pdf", mime: "application/pdf", size: 1024, dataBase64: "cGRm" },
    ]);
    expect(result.skipped).toBe(0);
  });

  it("keeps an oversized inline image as a file attachment", async () => {
    const oversized = { ...inlineImage, size: INLINE_IMAGE_MAX_BYTES + 1 };
    const result = await buildForward(
      message({ attachments: [oversized, fileAttachment] }),
      fetcher({ 0: "aW1n", 1: "cGRm" }),
    );

    expect(result.html).not.toContain("cid:logo@example.com");
    expect(result.attachments.map((attachment) => attachment.name)).toEqual([
      "logo.png",
      "report.pdf",
    ]);
    expect(result.skipped).toBe(1);
  });

  it("copies unused inline parts and reports failed fetches", async () => {
    const result = await buildForward(
      message({ html: "<p>No image here</p>" }),
      fetcher({ 0: "aW1n" }),
    );

    expect(result.attachments.map((attachment) => attachment.name)).toEqual(["logo.png"]);
    expect(result.skipped).toBe(1);
  });

  it("removes unresolvable cid images", async () => {
    const result = await buildForward(
      message({ html: '<p><img src="cid:missing@example.com"></p>' }),
      fetcher({ 0: "aW1n", 1: "cGRm" }),
    );

    expect(result.html).not.toContain("cid:missing@example.com");
    expect(result.attachments.map((attachment) => attachment.name)).toEqual([
      "logo.png",
      "report.pdf",
    ]);
    expect(result.skipped).toBe(0);
  });

  it("quotes a text-only message", async () => {
    const result = await buildForward(
      message({ html: null, text: "line 1\nline 2", attachments: [] }),
      fetcher({}),
    );

    expect(result.html).toContain("<blockquote>line 1<br>line 2</blockquote>");
    expect(result.attachments).toEqual([]);
  });
});
