import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listOutbox: vi.fn(),
  retryOutbox: vi.fn(),
  reopenOutboxEntry: vi.fn(),
  pollAccountErrors: vi.fn(),
  app: {
    value: {
      outboxAccountId: "account-1" as string | null,
      accounts: [{ id: "account-1", name: "Test" }],
      theme: "system" as const,
    },
  },
}));

vi.mock("./api", () => ({
  api: {
    listOutbox: mocks.listOutbox,
    retryOutbox: mocks.retryOutbox,
    reopenOutboxEntry: mocks.reopenOutboxEntry,
  },
}));
vi.mock("./stores.svelte", () => ({
  app: mocks.app,
  pollAccountErrors: mocks.pollAccountErrors,
}));

import Outbox from "./Outbox.svelte";

describe("Outbox", () => {
  beforeEach(() => {
    mocks.app.value.outboxAccountId = "account-1";
    mocks.listOutbox.mockReset().mockResolvedValue([
      {
        id: 7,
        kind: "Send",
        detail: "Send a queued message",
        createdAt: 0,
        attempts: 3,
        lastError: "auth failed",
        failedAt: 123,
      },
    ]);
    mocks.reopenOutboxEntry.mockReset().mockResolvedValue(undefined);
  });

  it("failed_rows_show_retry", async () => {
    render(Outbox);

    expect(await screen.findByText("Failed")).toBeInTheDocument();
    const retry = screen.getByRole("button", { name: "Retry" });
    await fireEvent.click(retry);
    expect(mocks.reopenOutboxEntry).toHaveBeenCalledWith(7);
    await waitFor(() => expect(mocks.listOutbox).toHaveBeenCalledTimes(2));
  });
});
