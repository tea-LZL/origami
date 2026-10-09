import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  editorFocus: vi.fn(),
}));

vi.mock("@tiptap/core", () => ({
  Editor: class MockEditor {
    commands = { focus: mocks.editorFocus };

    constructor(options: { onUpdate: (value: { editor: MockEditor }) => void }) {
      void options;
    }

    chain() {
      return {
        focus: () => ({ run: mocks.editorFocus }),
      };
    }

    getHTML() {
      return "<p></p>";
    }

    destroy() {}
  },
}));
vi.mock("@tiptap/starter-kit", () => ({ default: { configure: () => ({}) } }));
vi.mock("@tiptap/extension-image", () => ({ default: {} }));
vi.mock("./stores.svelte", () => import("./composer-test-store.svelte"));

import ComposePane from "./ComposePane.svelte";
import {
  app,
  flushComposerSave,
  makeSession,
  minimizeComposer,
  resetComposerTestStore,
  scheduleComposerSave,
  sendComposer,
  syncComposerNow,
} from "./composer-test-store.svelte";

function fakeFile(name: string, size: number): File {
  return {
    name,
    type: "application/octet-stream",
    size,
    arrayBuffer: async () => new ArrayBuffer(0),
  } as unknown as File;
}

function dropFile(target: Element, file: File) {
  const event = new Event("drop", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { files: [file] } });
  target.dispatchEvent(event);
}

describe("ComposePane", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetComposerTestStore();
  });

  it("focuses To for a new session", async () => {
    render(ComposePane);

    await waitFor(() => expect(screen.getByLabelText("To")).toHaveFocus());
  });

  it("renders the active session subject", () => {
    render(ComposePane);

    expect(screen.getByLabelText("Subject")).toHaveValue("Draft subject");
  });

  it("minimizes from the header button", async () => {
    render(ComposePane);

    await fireEvent.click(screen.getByRole("button", { name: "Minimize composer" }));

    expect(minimizeComposer).toHaveBeenCalledTimes(1);
  });

  it("shows_saved_locally_for_saved_session", () => {
    app.value.composerSessions = [
      makeSession("session-1", { saveState: "saved", lastSavedAt: Date.now() }),
    ];
    render(ComposePane);

    expect(screen.getByText(/Saved locally/)).toBeInTheDocument();
  });

  it("schedules an autosave for the session", async () => {
    render(ComposePane);

    await waitFor(() => expect(scheduleComposerSave).toHaveBeenCalledWith("session-1"));
  });

  it("flushes on beforeunload", () => {
    render(ComposePane);

    window.dispatchEvent(new Event("beforeunload"));

    expect(flushComposerSave).toHaveBeenCalledWith("session-1");
  });

  it("shows_sync_retry_on_error", async () => {
    app.value.composerSessions = [makeSession("session-1", { syncState: "error" })];
    render(ComposePane);

    await fireEvent.click(screen.getByRole("button", { name: /Retry/i }));

    expect(syncComposerNow).toHaveBeenCalledWith("session-1");
  });

  it("drop_adds_attachment", async () => {
    render(ComposePane);

    dropFile(document.querySelector(".compose-pane")!, fakeFile("notes.txt", 12));

    await waitFor(() => expect(screen.getByText("notes.txt")).toBeInTheDocument());
  });

  it("attach_at_25mb_boundary", async () => {
    const limit = 25 * 1024 * 1024;
    render(ComposePane);
    const pane = document.querySelector(".compose-pane")!;

    dropFile(pane, fakeFile("exact.bin", limit));
    await waitFor(() => expect(screen.getByText("exact.bin")).toBeInTheDocument());

    dropFile(pane, fakeFile("over.bin", 1));
    await waitFor(() => expect(app.value.lastError).toContain("25 MB"));
    expect(screen.queryByText("over.bin")).not.toBeInTheDocument();
  });

  it("cc_bcc_hidden_until_toggled", async () => {
    render(ComposePane);
    expect(screen.queryByLabelText("Cc")).not.toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Cc/Bcc" }));

    expect(screen.getByLabelText("Cc")).toBeInTheDocument();
    expect(screen.getByLabelText("Bcc")).toBeInTheDocument();
  });

  it("ctrl_enter_sends", async () => {
    render(ComposePane);

    await fireEvent.keyDown(document.querySelector(".compose-pane")!, {
      key: "Enter",
      ctrlKey: true,
    });

    expect(sendComposer).toHaveBeenCalledWith("session-1");
  });

  it("ctrl_enter_ignored_outside_pane", async () => {
    render(ComposePane);
    const outside = document.createElement("input");
    document.body.appendChild(outside);
    outside.focus();

    await fireEvent.keyDown(outside, { key: "Enter", ctrlKey: true });

    expect(sendComposer).not.toHaveBeenCalled();
    outside.remove();
  });
});
