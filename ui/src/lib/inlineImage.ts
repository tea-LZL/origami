/**
 * Inline image embedding for the composer: files become data-URL images in
 * the HTML body (reading-side sanitizer already allows `img-src data:`).
 */

export const INLINE_IMAGE_MAX_BYTES = 5 * 1024 * 1024;

export class InlineImageError extends Error {}

export function isInlineImage(file: File): boolean {
  return file.type.startsWith("image/");
}

export async function fileToInlineImage(file: File): Promise<string> {
  if (!isInlineImage(file)) {
    throw new InlineImageError(`"${file.name}" is not an image`);
  }
  if (file.size > INLINE_IMAGE_MAX_BYTES) {
    throw new InlineImageError(
      `"${file.name}" exceeds the ${Math.round(INLINE_IMAGE_MAX_BYTES / 1024 / 1024)} MB inline image limit`,
    );
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  const base64 = btoa(binary);
  return `data:${file.type};base64,${base64}`;
}
