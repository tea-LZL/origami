import { describe, expect, it } from "vitest";
import { TAG_PALETTE, tagColor } from "./tags";

describe("tagColor", () => {
  it("returns a stable palette token for a name", () => {
    expect(tagColor("receipts")).toBe(tagColor("receipts"));
    expect(tagColor("receipts")).toMatch(/^var\(--tag-/);
  });

  it("tag_palette_has_twelve", () => {
    expect(TAG_PALETTE).toHaveLength(12);
  });

  it("tag_color_stable_and_uniform", () => {
    const slots = new Set<string>();
    for (let i = 0; i < 500; i++) {
      const name = `tag-${i}`;
      expect(tagColor(name)).toBe(tagColor(name));
      slots.add(tagColor(name));
    }
    expect(slots.size).toBeGreaterThanOrEqual(8);
  });
});
