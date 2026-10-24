import type { Envelope } from "./types";

/** Everything the incoming-row decision needs from the current list view. */
export interface IncomingView {
  envelopes: Envelope[];
  unreadOnly: boolean;
  searchActive: boolean;
  page: number;
  unifiedInbox: boolean;
  /** The event's folder belongs to the current view. */
  relevant: boolean;
}

export interface IncomingResult {
  envelopes: Envelope[];
  inserted: boolean;
}

const isSeen = (envelope: Envelope) => envelope.flags.includes("Seen");
const receivedKey = (envelope: Envelope) => envelope.receivedAt ?? 0;

function physicalSources(envelope: Envelope) {
  if (envelope.sources.length > 0) return envelope.sources;
  return envelope.serverUid == null
    ? []
    : [{ mailboxId: envelope.mailboxId, serverUid: envelope.serverUid }];
}

/**
 * Insert a pushed envelope into the visible page without a refetch.
 *
 * A miss is normal: searches, later pages, unrelated folders, already-present
 * logical messages (same row id or the same physical source), and Seen rows
 * under the unread-only filter all fall back to the authoritative
 * `folderSynced` reload. Received-time order matches the store's
 * `received_at DESC, rowid DESC`; a brand-new row wins a tie.
 */
export function insertIncomingEnvelope(
  view: IncomingView,
  incoming: Envelope,
): IncomingResult {
  if (!incoming.id || view.searchActive || view.page !== 1 || !view.relevant) {
    return { envelopes: view.envelopes, inserted: false };
  }
  if (view.unreadOnly && isSeen(incoming)) {
    return { envelopes: view.envelopes, inserted: false };
  }
  const incomingSources = physicalSources(incoming);
  const alreadyPresent = view.envelopes.some((envelope) => {
    if (envelope.id === incoming.id) return true;
    return physicalSources(envelope).some((existing) =>
      incomingSources.some(
        (source) =>
          source.mailboxId === existing.mailboxId &&
          source.serverUid === existing.serverUid,
      ),
    );
  });
  if (alreadyPresent) {
    return { envelopes: view.envelopes, inserted: false };
  }
  const index = view.envelopes.findIndex(
    (envelope) => receivedKey(envelope) <= receivedKey(incoming),
  );
  const envelopes = [...view.envelopes];
  envelopes.splice(index === -1 ? envelopes.length : index, 0, incoming);
  return { envelopes, inserted: true };
}
