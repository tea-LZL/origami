import type { Envelope } from "./types";

export function threadKey(envelope: Envelope): string {
  if (envelope.threadId) return envelope.threadId;
  const subject = envelope.subject
    .replace(/^\s*((re|fw|fwd)\s*:\s*)+/i, "")
    .replace(/\s+/g, " ")
    .trim()
    .toLocaleLowerCase();
  return subject || envelope.messageId || envelope.id;
}

export function threadMembers(envelopes: Envelope[], selected: Envelope | null): Envelope[] {
  if (!selected) return [];
  const key = threadKey(selected);
  return envelopes.filter((envelope) => threadKey(envelope) === key);
}
