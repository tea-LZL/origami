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
    "style",
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

/** Sanitize mail HTML and remove network-backed image resources by default. */
export function sanitizeMessageHtml(raw: string, policy: MessageHtmlPolicy = {}): SanitizedMessageHtml {
  const sourceDocument = new DOMParser().parseFromString(raw, "text/html");
  const styleBlocks = [...sourceDocument.querySelectorAll("style")].map((style) => ({
    css: protectStyleText(style.textContent ?? ""),
    media: style.getAttribute("media"),
  }));
  const clean = DOMPurify.sanitize(raw, sanitizationOptions);
  const document = new DOMParser().parseFromString(clean, "text/html");
  for (const styleBlock of styleBlocks.reverse()) {
    const style = document.createElement("style");
    if (styleBlock.media) style.setAttribute("media", styleBlock.media);
    style.textContent = styleBlock.css;
    document.body.prepend(style);
  }
  const blockedHosts = new Set<string>();
  const blockedOrigins = new Set<string>();
  const imageSources = new Set(["data:", "cid:"]);
  let blockedResources = 0;

  for (const image of document.querySelectorAll<HTMLImageElement>("img[src]")) {
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
      imageSources.add(policy.allowRemoteImages ? url.protocol : origin);
      continue;
    }

    blockedResources += 1;
    if (url.hostname) blockedHosts.add(url.hostname);
    blockedOrigins.add(origin);
    image.removeAttribute("src");
    image.alt ||= "Remote image blocked";
    image.classList.add("remote-image-blocked");
  }

  if (policy.allowRemoteImages) {
    imageSources.add("http:");
    imageSources.add("https:");
  } else {
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

  const html = document.body.innerHTML;

  return {
    html,
    srcdoc: emailDocument(html, imageSources),
    blockedResources,
    blockedHosts: [...blockedHosts].sort(),
    blockedOrigins: [...blockedOrigins].sort(),
  };
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
    + `</head><body>${html}</body></html>`;
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
