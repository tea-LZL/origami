import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  app: {
    value: {
      density: "comfortable",
      layout: "three-pane",
      searchQuery: "",
      unifiedInbox: false,
      folders: [] as Array<{
        id: string;
        accountId: string;
        name: string;
        role: string;
        total: number;
        unread: number;
        sourceIds: string[];
      }>,
      selectedFolderId: null as string | null,
      selectedMessageIds: [] as string[],
      envelopes: [] as unknown[],
      envelopesLoading: false,
      envelopesLoadingMore: false,
      searching: false,
      savedSearches: [] as unknown[],
      selectedEnvelope: null as unknown,
      unreadOnly: false,
      theme: "system" as const,
      selectingAll: false,
      composerOpen: false,
    },
  },
  clearMessageSelection: vi.fn(),
  deleteSelectedPermanently: vi.fn(),
  loadMoreEnvelopes: vi.fn(),
  moveSelectedToFolder: vi.fn(),
  moveSelectedToRole: vi.fn(),
  selectAllMessages: vi.fn(),
  selectEnvelope: vi.fn(),
  selectEnvelopeExclusive: vi.fn(),
  searchMessages: vi.fn(),
  saveCurrentSearch: vi.fn(),
  removeSavedSearch: vi.fn(),
  setLayout: vi.fn(),
  setSelectedFlag: vi.fn(),
  setSelectedKeyword: vi.fn(),
  toggleEnvelopeSelection: vi.fn(),
  recordFolderScroll: vi.fn(),
  restoredFolderScroll: vi.fn(() => null),
  setUnreadOnly: vi.fn(),
}));

vi.mock("./stores.svelte", () => mocks);

import ThreadList from "./ThreadList.svelte";

describe("ThreadList", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
      value: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
        removeItem: (key: string) => values.delete(key),
        clear: () => values.clear(),
      },
      configurable: true,
    });
    mocks.app.value = {
      density: "comfortable",
      layout: "three-pane",
      searchQuery: "",
      unifiedInbox: false,
      folders: [],
      selectedFolderId: null,
      selectedMessageIds: [],
      envelopes: [],
      envelopesLoading: false,
      envelopesLoadingMore: false,
      searching: false,
      savedSearches: [],
      selectedEnvelope: null,
      unreadOnly: false,
      theme: "system",
      selectingAll: false,
      composerOpen: false,
    };
    mocks.setUnreadOnly = vi.fn();
  });

  it("toggles unread-only and reports pressed state", async () => {
    mocks.setUnreadOnly = vi.fn(async (value: boolean) => {
      mocks.app.value.unreadOnly = value;
    });
    render(ThreadList);
    const toggle = screen.getByRole("button", { name: "Unread" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(toggle);
    expect(mocks.setUnreadOnly).toHaveBeenCalledWith(true);
  });

  it("uses unread empty copy and folder unread count when filtering", () => {
    mocks.app.value.unreadOnly = true;
    mocks.app.value.envelopes = [];
    mocks.app.value.selectedFolderId = "inbox";
    mocks.app.value.folders = [{
      id: "inbox", accountId: "a", name: "INBOX", role: "Inbox",
      total: 12, unread: 3, sourceIds: [],
    }];
    render(ThreadList);
    expect(screen.getByText("No unread messages")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});
