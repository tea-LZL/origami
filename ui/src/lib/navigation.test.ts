import { afterEach, describe, expect, it } from "vitest";
import {
  focusNavigationTarget,
  handleMessageBodyKeydown,
  handleSidebarKeydown,
} from "./navigation";

afterEach(() => {
  document.body.innerHTML = "";
});

describe("focusNavigationTarget", () => {
  it("focuses selected mailbox for Alt+1", () => {
    document.body.innerHTML = `
      <aside data-navigation="sidebar">
        <button class="unified">Unified Inbox</button>
        <button class="folder selected">Inbox</button>
      </aside>
    `;

    expect(focusNavigationTarget("1", null)).toBe(true);
    expect(document.activeElement).toHaveClass("selected");
  });

  it("focuses selected message row or message list for Alt+2", () => {
    document.body.innerHTML = `
      <div data-navigation="message-list" tabindex="0">
        <div data-item-id="first" tabindex="-1"></div>
        <div data-item-id="second" tabindex="-1"></div>
      </div>
    `;

    expect(focusNavigationTarget("2", "second")).toBe(true);
    expect((document.activeElement as HTMLElement).dataset.itemId).toBe("second");

    document.body.innerHTML = '<div data-navigation="message-list" tabindex="0"></div>';
    expect(focusNavigationTarget("2", "missing")).toBe(true);
    expect(document.activeElement).toHaveAttribute("data-navigation", "message-list");
  });

  it("focuses message body for Alt+3", () => {
    document.body.innerHTML = '<div data-navigation="message-body" tabindex="-1"></div>';

    expect(focusNavigationTarget("3", null)).toBe(true);
    expect(document.activeElement).toHaveAttribute("data-navigation", "message-body");
  });

  it("moves sidebar selection with J/K", () => {
    document.body.innerHTML = `
      <aside data-navigation="sidebar">
        <button class="unified selected">Unified Inbox</button>
        <button class="folder">Inbox</button>
      </aside>
    `;
    const sidebar = document.querySelector<HTMLElement>("[data-navigation=sidebar]")!;
    const buttons = sidebar.querySelectorAll<HTMLButtonElement>("button");
    buttons[0].focus();

    const event = new KeyboardEvent("keydown", { key: "j", bubbles: true, cancelable: true });
    sidebar.addEventListener("keydown", (keydown) => handleSidebarKeydown(keydown, sidebar));
    sidebar.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(buttons[1]);
  });

  it("scrolls message body with J/K", () => {
    const body = document.createElement("div");
    Object.defineProperty(body, "clientHeight", { value: 500 });
    body.scrollTop = 100;

    const down = new KeyboardEvent("keydown", { key: "j", cancelable: true });
    handleMessageBodyKeydown(down, body);
    expect(down.defaultPrevented).toBe(true);
    expect(body.scrollTop).toBe(500);

    const up = new KeyboardEvent("keydown", { key: "k", cancelable: true });
    handleMessageBodyKeydown(up, body);
    expect(body.scrollTop).toBe(100);
  });
});
