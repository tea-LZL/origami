const IMAGE_PREVIEW_LIMIT = 8 * 1024 * 1024;
const TEXT_PREVIEW_LIMIT = 512 * 1024;
const IMAGE_MIME = /^image\/[a-z0-9.+-]+$/;

export type AttachmentAction = "image" | "text" | "open";

// Images and plain text stay inside the reading pane. Everything else is
// handed to the system handler.
export function attachmentAction(mime: string, size: number): AttachmentAction {
  const base = mime.split(";")[0]?.trim().toLowerCase() ?? "";
  if (IMAGE_MIME.test(base) && size >= 0 && size <= IMAGE_PREVIEW_LIMIT) return "image";
  if ((base === "text/plain" || base === "text/csv") && size >= 0 && size <= TEXT_PREVIEW_LIMIT) {
    return "text";
  }
  return "open";
}

export function decodeBase64(value: string): Uint8Array {
  const cleaned = value.replace(/\s+/g, "");
  if (cleaned.length === 0 || cleaned.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(cleaned)) {
    throw new Error("invalid attachment data");
  }
  const binary = atob(cleaned);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// The MIME is reduced to a token so a hostile Content-Type cannot break
// out of the URL. Bytes are decoded first so the preview cannot exceed the cap.
export function imageDataUrl(mime: string, base64: string): string {
  const base = mime.split(";")[0]?.trim().toLowerCase() ?? "";
  if (!IMAGE_MIME.test(base)) throw new Error("not an image");
  const bytes = decodeBase64(base64);
  if (bytes.byteLength > IMAGE_PREVIEW_LIMIT) throw new Error("image is too large to preview");
  return `data:${base};base64,${base64.replace(/\s+/g, "")}`;
}

export function previewText(base64: string): string {
  const bytes = decodeBase64(base64);
  if (bytes.byteLength > TEXT_PREVIEW_LIMIT) throw new Error("text attachment is too large to preview");
  return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
}
