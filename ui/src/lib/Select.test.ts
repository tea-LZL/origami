import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import Select from "./Select.svelte";

const options = [
  { value: "system", label: "Follow system" },
  { value: "light", label: "Paper light" },
  { value: "dark", label: "Midnight blue" },
];

describe("Select", () => {
  it("selects an option and reports the value", async () => {
    const onValueChange = vi.fn();
    render(Select, {
      value: "system",
      options,
      ariaLabel: "Theme",
      onValueChange,
    });

    await fireEvent.click(screen.getByRole("button", { name: "Theme" }));
    await fireEvent.click(screen.getByRole("option", { name: "Midnight blue", hidden: true }));

    expect(onValueChange).toHaveBeenCalledWith("dark");
    expect(screen.getByRole("button", { name: "Theme" })).toHaveTextContent("Midnight blue");
  });

  it("supports arrow keys and Enter", async () => {
    const onValueChange = vi.fn();
    render(Select, {
      value: "system",
      options,
      ariaLabel: "Theme",
      onValueChange,
    });
    const trigger = screen.getByRole("button", { name: "Theme" });

    await fireEvent.keyDown(trigger, { key: "ArrowDown" });
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    await fireEvent.keyDown(document.activeElement!, { key: "Enter" });

    expect(onValueChange).toHaveBeenCalledWith("light");
    expect(trigger).toHaveFocus();
  });

  it("skips disabled options", async () => {
    const onValueChange = vi.fn();
    render(Select, {
      value: "system",
      options: [options[0], { ...options[1], disabled: true }, options[2]],
      ariaLabel: "Theme",
      onValueChange,
    });
    const trigger = screen.getByRole("button", { name: "Theme" });

    await fireEvent.keyDown(trigger, { key: "ArrowDown" });
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    await fireEvent.keyDown(document.activeElement!, { key: "Enter" });

    expect(onValueChange).toHaveBeenCalledWith("dark");
  });

  it("supports Home, End, typeahead, and Escape", async () => {
    const onValueChange = vi.fn();
    render(Select, {
      value: "system",
      options,
      ariaLabel: "Theme",
      onValueChange,
    });
    const trigger = screen.getByRole("button", { name: "Theme" });
    await fireEvent.click(trigger);

    const menu = screen.getByRole("listbox", { hidden: true });
    await fireEvent.keyDown(menu, { key: "End" });
    expect(document.activeElement).toHaveTextContent("Midnight blue");
    await fireEvent.keyDown(menu, { key: "Enter" });
    expect(onValueChange).toHaveBeenCalledWith("dark");

    await fireEvent.click(trigger);
    await fireEvent.keyDown(menu, { key: "d" });
    await fireEvent.keyDown(menu, { key: "Escape" });
    expect(trigger).toHaveFocus();
  });
});
