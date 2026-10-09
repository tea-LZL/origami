import { describe, expect, it } from "vitest";
import { isNewMessageShortcut } from "./appShortcuts";

function key(overrides: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    key: "n",
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("isNewMessageShortcut", () => {
  it("matches Ctrl+N only", () => {
    expect(isNewMessageShortcut(key({ ctrlKey: true }))).toBe(true);
    expect(isNewMessageShortcut(key({ key: "N", ctrlKey: true }))).toBe(true);
    expect(isNewMessageShortcut(key())).toBe(false);
    expect(isNewMessageShortcut(key({ ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(isNewMessageShortcut(key({ ctrlKey: true, altKey: true }))).toBe(false);
    expect(isNewMessageShortcut(key({ metaKey: true }))).toBe(false);
  });
});
