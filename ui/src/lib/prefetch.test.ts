import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("./api", () => ({
  api: { prefetchDisplay: vi.fn().mockResolvedValue(undefined) },
}));

import { createPrefetcher } from "./prefetch";
import { api } from "./api";

const mocked = vi.mocked(api.prefetchDisplay);

function envelope(id: string, serverUid: number) {
  return {
    id,
    mailboxId: "folder-1",
    serverUid,
  } as Parameters<ReturnType<typeof createPrefetcher>["hover"]>[0];
}

describe("createPrefetcher", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocked.mockClear();
    mocked.mockResolvedValue(undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces_sweeps", async () => {
    const prefetcher = createPrefetcher();
    prefetcher.hover(envelope("a", 1));
    prefetcher.hover(envelope("b", 2));
    prefetcher.hover(envelope("c", 3));
    await vi.advanceTimersByTimeAsync(150);

    expect(mocked).toHaveBeenCalledTimes(1);
    expect(mocked.mock.calls[0][0]).toHaveLength(3);
  });

  it("sends_predictive_priority", async () => {
    const prefetcher = createPrefetcher();
    prefetcher.hover(envelope("a", 1));
    await vi.advanceTimersByTimeAsync(150);

    expect(mocked).toHaveBeenCalledWith([
      { folderId: "folder-1", serverUid: 1, priority: "predictive" },
    ]);
  });

  it("cancel_drops_pending", async () => {
    const prefetcher = createPrefetcher();
    prefetcher.hover(envelope("a", 1));
    prefetcher.cancel();
    await vi.advanceTimersByTimeAsync(150);

    expect(mocked).not.toHaveBeenCalled();
  });
});

describe("createPrefetcher viewport", () => {
  const envelopes = Array.from({ length: 20 }, (_, i) => envelope(`e${i}`, i + 1));

  beforeEach(() => {
    vi.useFakeTimers();
    mocked.mockClear();
    mocked.mockResolvedValue(undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("viewport_enqueues_margin", async () => {
    const prefetcher = createPrefetcher();
    prefetcher.viewport(envelopes, { start: 5, end: 10 });
    await vi.advanceTimersByTimeAsync(200);

    expect(mocked).toHaveBeenCalledTimes(1);
    const requests = mocked.mock.calls[0][0];
    // Margin: one neighbor viewport each side — span 5 → rows 0..14.
    expect(requests).toHaveLength(15);
    expect(requests[0].serverUid).toBe(1);
    expect(requests[14].serverUid).toBe(15);
    expect(requests.every((r) => r.priority === "viewport")).toBe(true);
  });

  it("viewport_clamps_bounds", async () => {
    const prefetcher = createPrefetcher();
    prefetcher.viewport(envelopes, { start: 0, end: 2 });
    await vi.advanceTimersByTimeAsync(200);

    const requests = mocked.mock.calls[0][0];
    // span 2 → [0 - 2, 2 + 2) clamped to [0, 4): no negative indices.
    expect(requests.map((r) => r.serverUid)).toEqual([1, 2, 3, 4]);
  });

  it("viewport_not_spammy", async () => {
    const prefetcher = createPrefetcher();
    for (let i = 0; i < 5; i++) {
      prefetcher.viewport(envelopes, { start: i, end: i + 3 });
    }
    await vi.advanceTimersByTimeAsync(200);

    expect(mocked).toHaveBeenCalledTimes(1);
  });
});
