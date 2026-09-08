import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import TrapFocusHarness from "../test/TrapFocusHarness.svelte";

describe("trapFocus", () => {
  afterEach(cleanup);

  it("focuses the first control and cycles Tab inside the dialog", async () => {
    const outside = document.createElement("button");
    outside.textContent = "Outside";
    document.body.append(outside);
    outside.focus();

    const view = render(TrapFocusHarness);
    const one = screen.getByRole("button", { name: "One" });
    const two = screen.getByRole("button", { name: "Two" });
    await waitFor(() => expect(one).toHaveFocus());

    two.focus();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
    expect(one).toHaveFocus();

    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab", shiftKey: true });
    expect(two).toHaveFocus();

    view.unmount();
    expect(outside).toHaveFocus();
    outside.remove();
  });
});
