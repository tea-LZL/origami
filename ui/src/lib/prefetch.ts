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
  cancel(): void;
}

export function createPrefetcher(opts?: { debounceMs?: number }): Prefetcher {
  const debounceMs = opts?.debounceMs ?? 150;
  let pending: PrefetchRequest[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function enqueue(target: PrefetchTarget | null, priority: PrefetchPriority): void {
    if (!target || target.serverUid == null) return;
    pending.push({
      folderId: target.mailboxId,
      serverUid: target.serverUid,
      priority,
    });
    if (timer != null) clearTimeout(timer);
    timer = setTimeout(flush, debounceMs);
  }

  function flush(): void {
    timer = null;
    const batch = pending;
    pending = [];
    if (batch.length > 0) void api.prefetchDisplay(batch).catch(() => {});
  }

  return {
    hover(envelope) {
      enqueue(envelope, "predictive");
    },
    focusMove(envelope) {
      enqueue(envelope, "predictive");
    },
    cancel() {
      if (timer != null) clearTimeout(timer);
      timer = null;
      pending = [];
    },
  };
}
