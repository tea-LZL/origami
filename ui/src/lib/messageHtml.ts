import DOMPurify from "dompurify";

export interface SanitizedMessageHtml {
  html: string;
  srcdoc: string;
  blockedResources: number;
  blockedHosts: string[];
  blockedOrigins: string[];
}

export interface MessageHtmlPolicy {
  allowRemoteImages?: boolean;
  allowedOrigins?: ReadonlySet<string>;
}

const sanitizationOptions = {
  USE_PROFILES: { html: true },
  // Style blocks are preserved for newsletter layout. Their text is escaped
  // below before it reaches the sandboxed document; CSS resources remain
  // bound by the generated CSP.
  FORBID_TAGS: [
    "audio",
    "base",
    "button",
    "embed",
    "form",
    "iframe",
    "input",
    "link",
    "meta",
    "object",
    "script",
    "select",
    "textarea",
    "video",
  ],
  FORBID_ATTR: [
    "action",
    "formaction",
    "ping",
    "poster",
    "srcset",
  ],
  ALLOW_DATA_ATTR: false,
  ALLOW_UNKNOWN_PROTOCOLS: false,
};

const SANITIZED_CACHE_ENTRIES = 12;
const SANITIZED_CACHE_BYTES = 16 * 1024 * 1024;
const sanitizedCache = new Map<string, { value: SanitizedMessageHtml; bytes: number }>();
let sanitizedCacheBytes = 0;

function policyFingerprint(policy: MessageHtmlPolicy): string {
  const origins = [...(policy.allowedOrigins ?? [])].sort();
  return `${policy.allowRemoteImages ? "remote" : "blocked"}|${origins.join("\u0000")}`;
}

function sanitizedCacheKey(
  key: string,
  policy: MessageHtmlPolicy,
  rawLength: number,
): string {
  return `${key}\u0000${policyFingerprint(policy)}\u0000${rawLength}`;
}

function cachedSanitized(
  key: string | undefined,
  policy: MessageHtmlPolicy,
  rawLength: number,
): SanitizedMessageHtml | null {
  if (!key) return null;
  const cacheKey = sanitizedCacheKey(key, policy, rawLength);
  const entry = sanitizedCache.get(cacheKey);
  if (!entry) return null;
  // Refresh recency.
  sanitizedCache.delete(cacheKey);
  sanitizedCache.set(cacheKey, entry);
  return entry.value;
}

function storeSanitized(
  key: string | undefined,
  policy: MessageHtmlPolicy,
  rawLength: number,
  value: SanitizedMessageHtml,
): void {
  if (!key) return;
  const cacheKey = sanitizedCacheKey(key, policy, rawLength);
  const bytes = value.html.length + value.srcdoc.length;
  const previous = sanitizedCache.get(cacheKey);
  if (previous) sanitizedCacheBytes -= previous.bytes;
  sanitizedCache.set(cacheKey, { value, bytes });
  sanitizedCacheBytes += bytes;
  while (
    sanitizedCache.size > 1
    && (sanitizedCache.size > SANITIZED_CACHE_ENTRIES || sanitizedCacheBytes > SANITIZED_CACHE_BYTES)
  ) {
    const oldest = sanitizedCache.keys().next().value;
    if (oldest === undefined) break;
    const removed = sanitizedCache.get(oldest);
    sanitizedCache.delete(oldest);
    sanitizedCacheBytes -= removed?.bytes ?? 0;
  }
}

/** Sanitize mail HTML and remove network-backed image resources by default. */
export function sanitizeMessageHtml(
  raw: string,
  policy: MessageHtmlPolicy = {},
  cacheKey?: string,
): SanitizedMessageHtml {
  const cached = cachedSanitized(cacheKey, policy, raw.length);
  if (cached) return cached;

  // Extract style blocks before sanitizing; they are re-inserted at the top
  // with escaped closing tags. Everything else is sanitized in one DOM pass.
  const source = new DOMParser().parseFromString(raw, "text/html");
  const styleBlocks = [...source.querySelectorAll("style")].map((style) => ({
    css: protectStyleText(style.textContent ?? ""),
    media: style.getAttribute("media"),
  }));
  for (const style of source.querySelectorAll("style")) style.remove();

  const fragment = DOMPurify.sanitize(source.body.innerHTML, {
    ...sanitizationOptions,
    RETURN_DOM: true,
  });
  const document = window.document.implementation.createHTMLDocument("");
  const container = document.createElement("div");
  container.appendChild(fragment);

  for (const styleBlock of styleBlocks.reverse()) {
    const style = document.createElement("style");
    if (styleBlock.media) style.setAttribute("media", styleBlock.media);
    style.textContent = styleBlock.css;
    container.prepend(style);
  }

  const blockedHosts = new Set<string>();
  const blockedOrigins = new Set<string>();
  const imageSources = new Set(["data:", "cid:"]);
  if (policy.allowRemoteImages) {
    imageSources.add("http:");
    imageSources.add("https:");
  }
  let blockedResources = 0;

  for (const image of container.querySelectorAll<HTMLImageElement>("img[src]")) {
    const source = image.getAttribute("src");
    if (!source) continue;
    let url: URL;
    try {
      url = new URL(source, "https://origami.invalid");
    } catch {
      image.removeAttribute("src");
      image.alt ||= "Blocked image";
      blockedResources += 1;
      continue;
    }
    if (url.protocol !== "http:" && url.protocol !== "https:") continue;

    const origin = url.origin;
    if (policy.allowRemoteImages || policy.allowedOrigins?.has(origin)) {
      if (!policy.allowRemoteImages) imageSources.add(origin);
      continue;
    }

    blockedResources += 1;
    if (url.hostname) blockedHosts.add(url.hostname);
    blockedOrigins.add(origin);
    image.removeAttribute("src");
    image.alt ||= "Remote image blocked";
    image.classList.add("remote-image-blocked");
  }

  if (!policy.allowRemoteImages) {
    for (const allowedOrigin of policy.allowedOrigins ?? []) {
      try {
        const url = new URL(allowedOrigin);
        if (url.protocol === "http:" || url.protocol === "https:") {
          imageSources.add(url.origin);
        }
      } catch {
        // Ignore malformed persisted allowlist entries.
      }
    }
  }

  const html = container.innerHTML;
  const result: SanitizedMessageHtml = {
    html,
    srcdoc: emailDocument(html, imageSources),
    blockedResources,
    blockedHosts: [...blockedHosts].sort(),
    blockedOrigins: [...blockedOrigins].sort(),
  };
  storeSanitized(cacheKey, policy, raw.length, result);
  return result;
}

function emailDocument(html: string, imageSources: ReadonlySet<string>): string {
  const contentSecurityPolicy = [
    "default-src 'none'",
    "base-uri 'none'",
    "connect-src 'none'",
    "form-action 'none'",
    "frame-src 'none'",
    "font-src data:",
    `img-src ${[...imageSources].join(" ")}`,
    "media-src 'none'",
    "object-src 'none'",
    "script-src 'none'",
    "style-src 'unsafe-inline'",
  ].join("; ");

  return '<!doctype html><html><head><meta charset="utf-8">'
    + `<meta http-equiv="Content-Security-Policy" content="${escapeAttribute(contentSecurityPolicy)}">`
    + "<style>html,body{margin:0;padding:0}</style>"
    // Email styles are preserved inside the body for compatibility with
    // real-world newsletter markup. Apply the readable fallback after them
    // so a global `body { color: white }` cannot produce white-on-white text.
    + `</head><body>${html}`
    + '<style id="origami-email-defaults">'
    + "html,body{margin:0;padding:0;overflow-x:auto !important;"
    + "color:#141d33 !important;"
    + "background-color:#fff !important;color-scheme:light}"
    + "</style></body></html>";
}

function escapeAttribute(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function protectStyleText(value: string): string {
  return value.replace(/<\/style/gi, "<\\/style");
}

const ALLOWED_LINK_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

/**
 * Normalized external href, or null when the scheme is not system-openable.
 * Rejects javascript:, file:, data:, vbscript:, and custom schemes — including
 * case and ASCII tab/newline obfuscations.
 */
export function allowedLinkHref(href: string): string | null {
  const cleaned = href.trim().replace(/[\t\n\r]/g, "");
  try {
    const url = new URL(cleaned);
    return ALLOWED_LINK_PROTOCOLS.has(url.protocol.toLowerCase()) ? url.href : null;
  } catch {
    return null;
  }
}
