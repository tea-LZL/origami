import { describe, expect, it } from "vitest";
import { attachmentAction, decodeBase64, imageDataUrl, previewText } from "./attachmentOpen";

describe("attachmentAction", () => {
  it("previews images and plain text inside the app", () => {
    expect(attachmentAction("image/png", 120)).toBe("image");
    expect(attachmentAction("image/svg+xml; charset=utf-8", 40)).toBe("image");
    expect(attachmentAction("text/plain", 20)).toBe("text");
    expect(attachmentAction("text/csv", 20)).toBe("text");
  });

  it("sends other files, oversized previews, and hostile types to the system handler", () => {
    expect(attachmentAction("application/pdf", 20)).toBe("open");
    expect(attachmentAction("application/zip", 20)).toBe("open");
    expect(attachmentAction("text/html", 20)).toBe("open");
    expect(attachmentAction("image/png", 9 * 1024 * 1024)).toBe("open");
    expect(attachmentAction('image/png" onerror="alert(1)', 20)).toBe("open");
    expect(attachmentAction("image/png\ntext/html", 20)).toBe("open");
  });
});

describe("imageDataUrl", () => {
  it("builds a data url from the mime token only", () => {
    expect(imageDataUrl("image/png; name=photo.png", "aGVsbG8=")).toBe(
      "data:image/png;base64,aGVsbG8=",
    );
  });

  it("rejects a mime that is not an image token", () => {
    expect(() => imageDataUrl('image/png"><img', "aGVsbG8=")).toThrow(/not an image/);
  });
});

describe("previewText", () => {
  it("decodes utf-8 text", () => {
    expect(previewText("aGVsbG8gd29ybGQ=")).toBe("hello world");
    expect(decodeBase64("aGVsbG8=")).toEqual(new Uint8Array([104, 101, 108, 108, 111]));
  });

  it("rejects malformed base64", () => {
    expect(() => previewText("!!!!")).toThrow(/invalid attachment data/);
    expect(() => previewText("abc")).toThrow(/invalid attachment data/);
    expect(() => previewText("aGVs===")).toThrow(/invalid attachment data/);
  });
});
