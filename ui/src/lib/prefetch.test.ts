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
