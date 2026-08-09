import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/svelte";
import { afterEach } from "vitest";

afterEach(cleanup);

const nativeMatches = Element.prototype.matches;
Element.prototype.matches = function matches(selector: string): boolean {
  if (selector === ":popover-open") return this.hasAttribute("data-popover-open");
  return nativeMatches.call(this, selector);
};

function dispatchToggle(element: HTMLElement, state: "open" | "closed") {
  const event = new Event("toggle");
  Object.defineProperty(event, "newState", { value: state });
  element.dispatchEvent(event);
}

HTMLElement.prototype.showPopover = function showPopover() {
  this.setAttribute("data-popover-open", "");
  dispatchToggle(this, "open");
};

HTMLElement.prototype.hidePopover = function hidePopover() {
  this.removeAttribute("data-popover-open");
  dispatchToggle(this, "closed");
};

globalThis.requestAnimationFrame = (callback: FrameRequestCallback) => {
  callback(0);
  return 0;
};

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class {
    observe() {}
    disconnect() {}
    unobserve() {}
  } as typeof ResizeObserver;
}
