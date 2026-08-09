import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import MessageSelectCheckbox from "./MessageSelectCheckbox.svelte";

describe("MessageSelectCheckbox", () => {
  it("keeps native checkbox semantics and reports normal selection", async () => {
    const onToggle = vi.fn();
    render(MessageSelectCheckbox, {
      checked: false,
      label: "Select message from Ada: Project update",
      onToggle,
    });
    const checkbox = screen.getByRole("checkbox", {
      name: "Select message from Ada: Project update",
    });

    expect(checkbox).not.toBeChecked();
    await fireEvent.click(checkbox);
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("reports Shift range selection", async () => {
    const onToggle = vi.fn();
    render(MessageSelectCheckbox, {
      checked: true,
      label: "Select message",
      onToggle,
    });

    await fireEvent.click(screen.getByRole("checkbox"), { shiftKey: true });
    expect(onToggle).toHaveBeenCalledWith(true);
  });
});
