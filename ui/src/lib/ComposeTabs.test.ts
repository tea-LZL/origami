import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./stores.svelte", () => import("./composer-test-store.svelte"));

import ComposeTabs from "./ComposeTabs.svelte";
import {
  activateComposer,
  app,
  makeSession,
  minimizeComposer,
  requestDiscardComposer,
  resetComposerTestStore,
} from "./composer-test-store.svelte";

describe("ComposeTabs", () => {
  beforeEach(() => {
    resetComposerTestStore();
    app.value.activeComposerId = null;
    app.value.composerSessions = [
      makeSession("a", { draft: { to: "", cc: "", bcc: "", subject: "First", html: "<p></p>", composeMode: "rich" } }),
      makeSession("b", { draft: { to: "bob@example.org", cc: "", bcc: "", subject: "", html: "<p></p>", composeMode: "rich" } }),
    ];
  });

  it("renders a tab per session with subject or recipient titles", () => {
    render(ComposeTabs);

    expect(screen.getByTitle("First")).toBeInTheDocument();
    expect(screen.getByTitle("bob@example.org")).toBeInTheDocument();
  });

  it("activates an inactive tab", async () => {
    render(ComposeTabs);

    await fireEvent.click(screen.getByTitle("First"));

    expect(activateComposer).toHaveBeenCalledWith("a");
  });

  it("minimizes when the active tab is clicked", async () => {
    app.value.activeComposerId = "a";
    render(ComposeTabs);

    await fireEvent.click(screen.getByTitle("First"));

    expect(minimizeComposer).toHaveBeenCalledTimes(1);
  });

  it("requests discard for non-empty tabs", async () => {
    render(ComposeTabs);

    await fireEvent.click(screen.getByRole("button", { name: "Discard draft: First" }));

    expect(requestDiscardComposer).toHaveBeenCalledWith("a");
  });

  it("marks the active composer with aria-current", () => {
    app.value.activeComposerId = "a";
    render(ComposeTabs);

    expect(screen.getByTitle("First")).toHaveAttribute("aria-current", "true");
    expect(screen.getByTitle("bob@example.org")).not.toHaveAttribute("aria-current");
  });

  it("arrow_keys_rove_focus", async () => {
    app.value.activeComposerId = "a";
    render(ComposeTabs);
    const first = screen.getByTitle("First");
    const second = screen.getByTitle("bob@example.org");
    first.focus();

    await fireEvent.keyDown(first, { key: "ArrowRight" });

    expect(second).toHaveFocus();
  });

  it("status_dot_reports_sync_error", () => {
    app.value.composerSessions[1].syncState = "error";
    render(ComposeTabs);

    expect(screen.getByLabelText("Sync failed")).toBeInTheDocument();
  });
});
