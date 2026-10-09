import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { Correspondent } from "./api";
import RecipientInput from "./RecipientInput.svelte";

const correspondents: Correspondent[] = [
  { addr: "alice@example.org", name: "Alice", messageCount: 1 },
  { addr: "bob@example.org", name: null, messageCount: 1 },
];

describe("RecipientInput", () => {
  it("renders_existing_recipients_as_chips", () => {
    render(RecipientInput, {
      props: { value: "alice@example.org, bob@example.org", label: "To", correspondents },
    });

    const chips = screen.getByRole("list", { name: "To recipients" });
    expect(within(chips).getByText("alice@example.org")).toBeInTheDocument();
    expect(within(chips).getByText("bob@example.org")).toBeInTheDocument();
    expect(within(chips).getByRole("button", { name: "Remove alice@example.org" })).toBeTruthy();
  });

  it("commits_on_enter_and_comma", async () => {
    const onChange = vi.fn();
    render(RecipientInput, {
      props: { value: "", label: "To", correspondents, onChange },
    });

    const input = screen.getByRole("textbox", { name: "To" });
    await fireEvent.input(input, { target: { value: "carol@example.org" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith("carol@example.org");
  });

  it("remove_button_clears_one_recipient", async () => {
    const onChange = vi.fn();
    render(RecipientInput, {
      props: { value: "alice@example.org, bob@example.org", label: "To", correspondents, onChange },
    });

    await fireEvent.click(screen.getByRole("button", { name: "Remove alice@example.org" }));

    expect(onChange).toHaveBeenCalledWith("bob@example.org");
  });

  it("commits_invalid_token_as_flagged_chip", async () => {
    const onChange = vi.fn();
    render(RecipientInput, {
      props: { value: "", label: "To", correspondents, onChange },
    });

    const input = screen.getByRole("textbox", { name: "To" });
    await fireEvent.input(input, { target: { value: "not-an-address" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).toHaveBeenCalledWith("not-an-address");
  });

  it("marks_invalid_chips_for_the_sender", () => {
    render(RecipientInput, {
      props: { value: "not-an-address", label: "To", correspondents },
    });

    const chip = screen.getByText("not-an-address");
    expect(chip.closest(".chip")).not.toBeNull();
    expect(chip).toHaveAttribute("aria-invalid", "true");
    expect(chip).toHaveAttribute("title", "Not a valid email address");
  });

  it("still_dedupes_already_present_recipients", async () => {
    const onChange = vi.fn();
    render(RecipientInput, {
      props: { value: "alice@example.org", label: "To", correspondents, onChange },
    });

    const input = screen.getByRole("textbox", { name: "To" });
    await fireEvent.input(input, { target: { value: "alice@example.org" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).not.toHaveBeenCalled();
  });

  it("backspace_removes_the_last_chip", async () => {
    const onChange = vi.fn();
    render(RecipientInput, {
      props: { value: "alice@example.org, bob@example.org", label: "To", correspondents, onChange },
    });

    const input = screen.getByRole("textbox", { name: "To" });
    await fireEvent.keyDown(input, { key: "Backspace" });

    expect(onChange).toHaveBeenCalledWith("alice@example.org");
  });
});
