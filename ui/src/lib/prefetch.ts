// Predictive + viewport display prefetch. Enqueues predicted rows so opening
// an email paints from cache; never touches flags (Rust side enforces).

import { api } from "./api";

type PrefetchPriority = "open" | "predictive" | "viewport";

export interface PrefetchTarget {
  mailboxId: string;
  serverUid: number | null;
}

export interface PrefetchRequest {
  folderId: string;
  serverUid: number;
  priority: PrefetchPriority;
}

export interface Prefetcher {
  hover(envelope: PrefetchTarget): void;
  focusMove(envelope: PrefetchTarget | null): void;
  viewport(envelopes: PrefetchTarget[], visible: { start: number; end: number }): void;
  cancel(): void;
}

export function createPrefetcher(opts?: {
  debounceMs?: number;
  viewportDebounceMs?: number;
}): Prefetcher {
  const debounceMs = opts?.debounceMs ?? 150;
  const viewportDebounceMs = opts?.viewportDebounceMs ?? 200;
  const rank: Record<PrefetchPriority, number> = { open: 4, predictive: 2, viewport: 1 };
  // Keyed by folder + UID so repeated hovers and overlapping viewport windows
  // collapse instead of queueing the same body many times.
  let pending = new Map<string, PrefetchRequest>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  function queue(target: PrefetchTarget | null, priority: PrefetchPriority): boolean {
    if (!target || target.serverUid == null) return false;
    const key = `${target.mailboxId}:${target.serverUid}`;
    const existing = pending.get(key);
    if (existing && rank[existing.priority] >= rank[priority]) return false;
    pending.set(key, {
      folderId: target.mailboxId,
      serverUid: target.serverUid,
      priority,
    });
    return true;
  }

  function schedule(delayMs: number): void {
    if (timer != null) clearTimeout(timer);
    timer = setTimeout(flush, delayMs);
  }

  function enqueue(
    target: PrefetchTarget | null,
    priority: PrefetchPriority,
    delayMs: number,
  ): void {
    if (queue(target, priority)) schedule(delayMs);
  }

  function flush(): void {
    timer = null;
    const batch = [...pending.values()];
    pending.clear();
    if (batch.length > 0) void api.prefetchDisplay(batch).catch(() => {});
  }

  return {
    hover(envelope) {
      enqueue(envelope, "predictive", debounceMs);
    },
    focusMove(envelope) {
      enqueue(envelope, "predictive", debounceMs);
    },
    viewport(envelopes, visible) {
      const span = Math.max(1, visible.end - visible.start);
      const start = Math.max(0, visible.start - span);
      const end = Math.min(envelopes.length, visible.end + span);
      // A scroll replaces the viewport window; appending every frame would
      // queue the whole visited range and fire it as one batch on settle.
      for (const [key, request] of pending) {
        if (request.priority === "viewport") pending.delete(key);
      }
      let added = false;
      for (const envelope of envelopes.slice(start, end)) {
        if (queue(envelope, "viewport")) added = true;
      }
      if (added) schedule(viewportDebounceMs);
    },
    cancel() {
      if (timer != null) clearTimeout(timer);
      timer = null;
      pending.clear();
    },
  };
}
