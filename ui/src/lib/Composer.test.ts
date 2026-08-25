import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const editorFocus = vi.fn();
  return {
    app: {
      value: {
        composerOpen: true,
        composerAccountId: "account-1",
        composerDraft: { to: "", cc: "", bcc: "", subject: "", html: "<p></p>" },
        composerAttachments: [],
        composerThreading: { inReplyTo: null, references: [] },
        correspondents: [],
        accounts: [{ id: "account-1", name: "Work", email: "me@example.org" }],
        sending: false,
      },
    },
    closeComposer: vi.fn(),
    sendComposer: vi.fn(),
    editorFocus,
  };
});

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
vi.mock("@tiptap/starter-kit", () => ({ default: {} }));
vi.mock("./stores.svelte", () => mocks);
vi.mock("./api", () => ({ api: { saveComposerDraft: vi.fn().mockResolvedValue(undefined) } }));

import Composer from "./Composer.svelte";

describe("Composer focus behavior", () => {
  beforeEach(() => {
    mocks.editorFocus.mockClear();
    mocks.app.value.composerOpen = true;
  });

  it("focuses To when opened", async () => {
    render(Composer);

    await waitFor(() => expect(screen.getByLabelText("To")).toHaveFocus());
  });

  it("focuses editor when clicking unused body space", async () => {
    render(Composer);
    const editor = document.querySelector<HTMLElement>(".editor");
    expect(editor).not.toBeNull();

    await fireEvent.click(editor!);
    expect(mocks.editorFocus).toHaveBeenCalled();
  });
});
