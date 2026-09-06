import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
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
vi.mock("@tiptap/starter-kit", () => ({ default: {} }));
vi.mock("./stores.svelte", () => import("./composer-test-store.svelte"));
vi.mock("./api", () => ({ api: { saveComposerDraft: vi.fn().mockResolvedValue(undefined) } }));

import Composer from "./Composer.svelte";
import { app, resetComposerTestStore } from "./composer-test-store.svelte";

describe("Composer focus behavior", () => {
  beforeEach(() => {
    mocks.editorFocus.mockClear();
    resetComposerTestStore();
  });

  it("focuses To when opened", async () => {
    render(Composer);

    await waitFor(() => expect(screen.getByLabelText("To")).toHaveFocus());
  });

  it("does not steal focus from other fields after opening", async () => {
    render(Composer);

    await waitFor(() => expect(screen.getByLabelText("To")).toHaveFocus());

    const subject = screen.getByLabelText("Subject");
    subject.focus();
    expect(subject).toHaveFocus();

    // Production `patch()` mutates fields in place (status poll, sync).
    Object.assign(app.value, { sending: false });
    await tick();
    expect(subject).toHaveFocus();

    // Replacing `app.value` must still not steal focus after mount.
    app.value = { ...app.value };

    await tick();
    expect(subject).toHaveFocus();
  });

  it("focuses editor when clicking unused body space", async () => {
    render(Composer);
    const editor = document.querySelector<HTMLElement>(".editor");
    expect(editor).not.toBeNull();

    await fireEvent.click(editor!);
    expect(mocks.editorFocus).toHaveBeenCalled();
  });
});
