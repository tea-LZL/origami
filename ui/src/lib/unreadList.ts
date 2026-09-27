import type { Envelope } from "./types";

export function folderViewKey(folderId: string, unreadOnly: boolean): string {
  return `${folderId}:${unreadOnly ? "unread" : "all"}`;
}

export function unreadKeepIds(
  selectedEnvelope: Envelope | null,
  selectedMessageIds: readonly string[],
): Set<string> {
  const keep = new Set(selectedMessageIds);
  if (selectedEnvelope) keep.add(selectedEnvelope.id);
  return keep;
}

export function retainForUnreadFilter(
  envelopes: Envelope[],
  unreadOnly: boolean,
  keepIds: Iterable<string>,
): Envelope[] {
  if (!unreadOnly) return envelopes;
  const keep = keepIds instanceof Set ? keepIds : new Set(keepIds);
  return envelopes.filter((envelope) => !envelope.flags.includes("Seen") || keep.has(envelope.id));
}

export function applyUnreadListReload(input: {
  loaded: Envelope[];
  previous: Envelope[];
  selectedEnvelope: Envelope | null;
  selectedMessageIds: readonly string[];
  unreadOnly: boolean;
}): { envelopes: Envelope[]; selectedMessageIds: string[] } {
  const keep = unreadKeepIds(input.selectedEnvelope, input.selectedMessageIds);
  if (!input.unreadOnly) {
    const available = new Set(input.loaded.map((envelope) => envelope.id));
    return {
      envelopes: input.loaded,
      selectedMessageIds: input.selectedMessageIds.filter((id) => available.has(id)),
    };
  }

  const envelopes = retainForUnreadFilter(
    mergeSelectedIntoUnreadPage(input.loaded, input.previous, input.selectedEnvelope, keep),
    true,
    keep,
  );
  const available = new Set(envelopes.map((envelope) => envelope.id));
  return {
    envelopes,
    selectedMessageIds: input.selectedMessageIds.filter((id) => available.has(id) || keep.has(id)),
  };
}

function mergeSelectedIntoUnreadPage(
  loaded: Envelope[],
  previous: Envelope[],
  selectedEnvelope: Envelope | null,
  keep: Set<string>,
): Envelope[] {
  if (keep.size === 0) return loaded;

  const loadedIds = new Set(loaded.map((envelope) => envelope.id));
  const extrasById = new Map<string, Envelope>();
  for (const envelope of previous) {
    if (keep.has(envelope.id) && !loadedIds.has(envelope.id)) {
      extrasById.set(envelope.id, envelope);
    }
  }
  // A selection carried from an earlier state must not overwrite an extra we
  // already took from `previous` — `previous` is newer than the selection
  // when the optimistic Seen patch has already been applied to the page.
  if (
    selectedEnvelope
    && keep.has(selectedEnvelope.id)
    && !loadedIds.has(selectedEnvelope.id)
    && !extrasById.has(selectedEnvelope.id)
  ) {
    extrasById.set(selectedEnvelope.id, selectedEnvelope);
  }
  if (extrasById.size === 0) return loaded;

  const result = loaded.slice();
  const previousIndex = new Map(previous.map((envelope, index) => [envelope.id, index]));
  const extras = [...extrasById.values()].sort(
    (a, b) =>
      (previousIndex.get(a.id) ?? Number.MAX_SAFE_INTEGER)
      - (previousIndex.get(b.id) ?? Number.MAX_SAFE_INTEGER),
  );
  for (const extra of extras) {
    const insertAt = Math.min(previousIndex.get(extra.id) ?? result.length, result.length);
    result.splice(insertAt, 0, extra);
  }
  return result;
}
