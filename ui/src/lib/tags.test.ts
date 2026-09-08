import { describe, expect, it } from "vitest";
import { tagColor } from "./tags";

describe("tagColor", () => {
  it("returns a stable palette token for a name", () => {
    expect(tagColor("receipts")).toBe(tagColor("receipts"));
    expect(tagColor("receipts")).toMatch(/^var\(--tag-/);
  });
});
