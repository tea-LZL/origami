import { fireEvent, render, screen, cleanup } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { app } from "./stores.svelte";
import ShortcutsDialog from "./ShortcutsDialog.svelte";

describe("ShortcutsDialog", () => {
  beforeEach(() => {
    app.value.shortcutsOpen = true;
  });

  afterEach(() => {
    app.value.shortcutsOpen = false;
    cleanup();
  });

  it("lists navigation keys and catalog shortcuts", () => {
    render(ShortcutsDialog);

    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    expect(screen.getByText("Move through the message list")).toBeInTheDocument();
    expect(screen.getByText("j / k, ↓ / ↑")).toBeInTheDocument();
    expect(screen.getByText("Archive")).toBeInTheDocument();
    expect(screen.getByText("Reply all")).toBeInTheDocument();
    expect(screen.getByText("Compose new message")).toBeInTheDocument();
    expect(screen.getByText("Open command palette")).toBeInTheDocument();
    expect(screen.getByText("Ctrl+K")).toBeInTheDocument();
  });

  it("closes from the close button", async () => {
    render(ShortcutsDialog);

    await fireEvent.click(screen.getByRole("button", { name: "Close keyboard shortcuts" }));

    expect(app.value.shortcutsOpen).toBe(false);
  });
});
