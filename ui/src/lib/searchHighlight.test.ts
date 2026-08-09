import { describe, expect, it } from "vitest";
import { highlightSearchText } from "./searchHighlight";

describe("highlightSearchText", () => {
  it("highlights free text and field-filter values", () => {
    expect(highlightSearchText("Ada project update", "from:ada update")).toEqual([
      { text: "Ada", match: true },
      { text: " project ", match: false },
      { text: "update", match: true },
    ]);
  });

  it("does not highlight non-text filter values or create markup", () => {
    expect(highlightSearchText("Read attachment", "is:read has:attachment")).toEqual([
      { text: "Read attachment", match: false },
    ]);
    expect(highlightSearchText("<b>safe</b>", "safe")).toEqual([
      { text: "<b>", match: false },
      { text: "safe", match: true },
      { text: "</b>", match: false },
    ]);
  });
});
