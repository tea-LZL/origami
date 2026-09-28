import { describe, expect, it } from "vitest";
import {
  fileToInlineImage,
  INLINE_IMAGE_MAX_BYTES,
  isInlineImage,
  InlineImageError,
} from "./inlineImage";

function file(name: string, type: string, size: number): File {
  return new File([new Uint8Array(size)], name, { type });
}

describe("inlineImage", () => {
  it("rejects_non_images", async () => {
    await expect(fileToInlineImage(file("doc.pdf", "application/pdf", 10))).rejects.toThrow(
      InlineImageError,
    );
    expect(isInlineImage(file("doc.pdf", "application/pdf", 10))).toBe(false);
  });

  it("rejects_oversized_images", async () => {
    await expect(
      fileToInlineImage(file("big.png", "image/png", INLINE_IMAGE_MAX_BYTES + 1)),
    ).rejects.toThrow(InlineImageError);
  });

  it("accepts_images_as_data_urls", async () => {
    const png = file("tiny.png", "image/png", 8);
    const url = await fileToInlineImage(png);
    expect(url).toMatch(/^data:image\/png;base64,/);
  });
});
