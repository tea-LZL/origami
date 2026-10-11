/**
 * Build a forwarded message body and attachment set from an opened message.
 *
 * The original HTML is sanitized before it is injected into the composer, and
 * inline `cid:` images are fetched and inlined as `data:` URLs so recipients
 * see them. Remote `http(s)` images are preserved for fidelity. Non-inline
 * attachments (and any inline part we could not inline) are copied as regular
 * attachments.
 */

import type { AttachmentMeta, MessageDto } from "./api";
import { formatMessageDate } from "./formatDate";
import { INLINE_IMAGE_MAX_BYTES } from "./inlineImage";
import { sanitizeMessageHtml } from "./messageHtml";

export interface ForwardAttachment {
  name: string;
  mime: string;
  size: number;
  dataBase64: string;
}

export interface ForwardResult {
  html: string;
  attachments: ForwardAttachment[];
  /** Parts that could not be fetched or inlined (reported to the user). */
  skipped: number;
}

export type AttachmentFetcher = (index: number) => Promise<string>;

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function normalizeCid(value: string): string {
  return value.trim().replace(/^<|>$/g, "").toLowerCase();
}

function addressText(addresses: { name: string | null; addr: string }[]): string {
  return addresses
    .map((address) => (address.name ? `${address.name} <${address.addr}>` : address.addr))
    .join(", ");
}

function attachmentName(attachment: AttachmentMeta): string {
  return attachment.name ?? `attachment-${attachment.index}`;
}

function forwardedHeaderBlock(message: MessageDto): string {
  const rows: [string, string][] = [
    ["From", addressText(message.envelope.from)],
    ["Date", formatMessageDate(message.envelope.date)],
    ["Subject", message.envelope.subject],
    ["To", addressText(message.envelope.to)],
  ];
  if (message.headers.cc.length > 0) {
    rows.push(["Cc", addressText(message.headers.cc)]);
  }
  const lines = rows
    .filter(([, value]) => value.trim().length > 0)
    .map(([label, value]) => `<b>${escapeHtml(label)}:</b> ${escapeHtml(value)}`)
    .join("<br>");
  return `<p></p><p>---------- Forwarded message ----------</p><p>${lines}</p>`;
}

async function buildHtmlBody(
  message: MessageDto,
  fetchAttachment: AttachmentFetcher,
): Promise<{ body: string; inlined: Set<number>; skipped: number }> {
  const safe = sanitizeMessageHtml(message.html ?? "", { allowRemoteImages: true }).html;
  const document = new DOMParser().parseFromString(`<body>${safe}</body>`, "text/html");
  const container = document.body;

  const byCid = new Map<string, AttachmentMeta>();
  for (const attachment of message.attachments) {
    if (attachment.inline && attachment.cid) {
      byCid.set(normalizeCid(attachment.cid), attachment);
    }
  }

  const serverUid = message.envelope.serverUid;
  const inlined = new Set<number>();
  let skipped = 0;
  const images = [...container.querySelectorAll<HTMLImageElement>("img[src]")];

  await Promise.all(images.map(async (image) => {
    const src = image.getAttribute("src") ?? "";
    if (!src.toLowerCase().startsWith("cid:")) return;
    const attachment = byCid.get(normalizeCid(src.slice(4)));
    if (!attachment) {
      image.remove();
      return;
    }
    if (serverUid == null || attachment.size > INLINE_IMAGE_MAX_BYTES) {
      image.remove();
      skipped += 1;
      return;
    }
    try {
      const dataBase64 = await fetchAttachment(attachment.index);
      image.setAttribute("src", `data:${attachment.mime};base64,${dataBase64}`);
      inlined.add(attachment.index);
    } catch {
      image.remove();
      skipped += 1;
    }
  }));

  return { body: container.innerHTML, inlined, skipped };
}

async function collectAttachments(
  message: MessageDto,
  fetchAttachment: AttachmentFetcher,
  inlined: Set<number>,
): Promise<{ attachments: ForwardAttachment[]; skipped: number }> {
  const serverUid = message.envelope.serverUid;
  let skipped = 0;
  const resolved = await Promise.all(
    message.attachments.map(async (attachment, order) => {
      if (inlined.has(attachment.index)) return null;
      if (serverUid == null) {
        skipped += 1;
        return null;
      }
      try {
        const dataBase64 = await fetchAttachment(attachment.index);
        return {
          order,
          attachment: {
            name: attachmentName(attachment),
            mime: attachment.mime || "application/octet-stream",
            size: attachment.size,
            dataBase64,
          } satisfies ForwardAttachment,
        };
      } catch {
        skipped += 1;
        return null;
      }
    }),
  );
  const attachments = resolved
    .filter((entry): entry is { order: number; attachment: ForwardAttachment } => entry !== null)
    .sort((a, b) => a.order - b.order)
    .map((entry) => entry.attachment);
  return { attachments, skipped };
}

async function buildForward(
  message: MessageDto,
  fetchAttachment: AttachmentFetcher,
): Promise<ForwardResult> {
  const header = forwardedHeaderBlock(message);
  const hasHtml = Boolean(message.html && message.html.trim());
  if (hasHtml) {
    const { body, inlined, skipped: inlineSkipped } =
      await buildHtmlBody(message, fetchAttachment);
    const { attachments, skipped: fileSkipped } =
      await collectAttachments(message, fetchAttachment, inlined);
    return { html: `${header}<p></p>${body}`, attachments, skipped: inlineSkipped + fileSkipped };
  }
  const { attachments, skipped } =
    await collectAttachments(message, fetchAttachment, new Set());
  const text = escapeHtml(message.text ?? "").replaceAll("\n", "<br>");
  return { html: `${header}<p></p><blockquote>${text}</blockquote>`, attachments, skipped };
}

export { buildForward };
