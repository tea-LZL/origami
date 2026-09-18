import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import VirtualListHarness from "../test/VirtualListHarness.svelte";

describe("VirtualList", () => {
  it("provides a keyboard focus entry point and listbox semantics", () => {
    render(VirtualListHarness);
    const list = screen.getByRole("listbox", { name: "Messages" });

    expect(list).toHaveAttribute("tabindex", "0");
    expect(list).toHaveAttribute("aria-multiselectable", "true");
    expect(list).toHaveAttribute("aria-keyshortcuts", "J K Enter Space");
    expect(screen.getAllByRole("option")).toHaveLength(2);
    list.focus();
    expect(list).toHaveFocus();
  });

  it("marks unread rows for styling", () => {
    render(VirtualListHarness);
    const rows = screen.getAllByRole("option");
    expect(rows[0]).toHaveClass("unread");
    expect(rows[1]).not.toHaveClass("unread");
  });
});
