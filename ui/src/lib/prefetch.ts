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
  let pending: PrefetchRequest[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function enqueue(
    target: PrefetchTarget | null,
    priority: PrefetchPriority,
    delayMs: number,
  ): void {
    if (!target || target.serverUid == null) return;
    pending.push({
      folderId: target.mailboxId,
      serverUid: target.serverUid,
      priority,
    });
    if (timer != null) clearTimeout(timer);
    timer = setTimeout(flush, delayMs);
  }

  function flush(): void {
    timer = null;
    const batch = pending;
    pending = [];
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
      for (const envelope of envelopes.slice(start, end)) {
        enqueue(envelope, "viewport", viewportDebounceMs);
      }
    },
    cancel() {
      if (timer != null) clearTimeout(timer);
      timer = null;
      pending = [];
    },
  };
}
