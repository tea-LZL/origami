import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const archiveRun = vi.fn();
  const inboxRun = vi.fn();
  return {
    archiveRun,
    inboxRun,
    app: {
      value: {
        commandPaletteOpen: true as boolean,
        selectedEnvelope: { id: "next" } as { id: string } | null,
      },
    },
    paletteItems: vi.fn((query: string) => {
      const trimmed = query.trim();
      if (!trimmed) {
        return [
          {
            id: "archive",
            title: "Archive",
            group: "Triage",
            chord: "E",
            enabled: true,
            run: archiveRun,
          },
          {
            id: "undo",
            title: "Undo",
            group: "Triage",
            chord: "Z",
            enabled: false,
            run: vi.fn(),
          },
        ];
      }
      if (trimmed.toLowerCase().includes("inb")) {
        return [
          {
            id: "go-folder-inbox",
            title: "Go to Work / Inbox",
            group: "Folders",
            chord: null,
            enabled: true,
            run: inboxRun,
          },
        ];
      }
      return [];
    }),
  };
});

vi.mock("./commands", () => ({
  paletteItems: mocks.paletteItems,
}));

vi.mock("./stores.svelte", () => ({
  app: mocks.app,
}));

import CommandPalette from "./CommandPalette.svelte";

describe("CommandPalette", () => {
  beforeEach(() => {
    mocks.archiveRun.mockReset();
    mocks.inboxRun.mockReset();
    mocks.app.value.commandPaletteOpen = true;
    mocks.app.value.selectedEnvelope = { id: "next" };
  });

  afterEach(cleanup);

  it("renders_dialog_and_closes_on_Escape_and_overlay_click", async () => {
    const { container } = render(CommandPalette);
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    const box = screen.getByRole("combobox", { name: "Command" });
    await waitFor(() => expect(box).toHaveFocus());

    const overlay = container.querySelector(".overlay");
    expect(overlay).toBeTruthy();
    await fireEvent.keyDown(overlay!, { key: "Escape" });
    expect(mocks.app.value.commandPaletteOpen).toBe(false);

    mocks.app.value.commandPaletteOpen = true;
    cleanup();
    const second = render(CommandPalette);
    await fireEvent.click(second.container.querySelector(".overlay")!);
    expect(mocks.app.value.commandPaletteOpen).toBe(false);
  });

  it("empty_query_shows_grouped_cheat_sheet_with_chords", async () => {
    render(CommandPalette);
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByText("Triage")).toBeInTheDocument();
    const archive = screen.getByRole("option", { name: /Archive/ });
    expect(archive).toHaveTextContent("E");
    expect(archive).toHaveAttribute("aria-selected", "true");
    const undo = screen.getByRole("option", { name: /Undo/ });
    expect(undo).toBeInTheDocument();
    expect(undo).toHaveAttribute("aria-disabled", "true");
    expect(undo).toHaveAttribute("aria-selected", "false");
  });

  it("typing_filters_and_Enter_runs_highlighted_and_closes", async () => {
    const row = document.createElement("button");
    row.setAttribute("data-item-id", "next");
    document.body.append(row);

    render(CommandPalette);
    const box = screen.getByRole("combobox");
    await fireEvent.input(box, { target: { value: "inb" } });
    expect(screen.getByRole("option", { name: /Go to Work \/ Inbox/ })).toBeInTheDocument();
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(mocks.inboxRun).toHaveBeenCalledTimes(1);
    expect(mocks.app.value.commandPaletteOpen).toBe(false);
    await waitFor(() => expect(row).toHaveFocus());
    row.remove();
  });
});
