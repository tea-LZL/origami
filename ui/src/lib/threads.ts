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

/**
 * Merge the loaded-page thread members with cross-folder query results.
 * Loaded entries win on id collisions; cross-folder rows arrive newest-first
 * and are appended in that order after the locals.
 */
export function mergeThreadMembers(local: Envelope[], cross: Envelope[]): Envelope[] {
  const seen = new Set(local.map((envelope) => envelope.id));
  const extras = cross.filter((envelope) => !seen.has(envelope.id));
  return [...local, ...extras];
}

/**
 * Merge the loaded-page thread members with cross-folder query results.
 * Loaded entries win on id collisions; cross-folder rows arrive newest-first
 * and are appended in that order after the locals.
 */
