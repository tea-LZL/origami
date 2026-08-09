import { describe, expect, it } from "vitest";
import { sanitizeMessageHtml } from "./messageHtml";

describe("sanitizeMessageHtml", () => {
  it("removes active content and blocks remote image requests", () => {
    const result = sanitizeMessageHtml(
      '<script>alert(1)</script><style>body{color:red}</style>'
        + '<img src="https://tracking.example/pixel" />'
        + '<img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP" />'
        + '<a href="https://example.org">Read more</a>',
    );

    expect(result.html).not.toContain("script");
    expect(result.html).not.toContain("tracking.example/pixel");
    expect(result.html).toContain("Read more");
    expect(result.blockedResources).toBe(1);
    expect(result.blockedHosts).toEqual(["tracking.example"]);
    expect(result.blockedOrigins).toEqual(["https://tracking.example"]);
  });

  it("preserves presentation CSS used by HTML email layouts", () => {
    const result = sanitizeMessageHtml(
      '<style>.preheader{display:none}.copy{color:#333}</style>'
        + '<div class="preheader">Manage your account</div>'
        + '<table style="width:600px;background:#fff"><tr>'
        + '<td class="copy" style="font-size:16px">Message</td></tr></table>',
    );

    expect(result.html).toContain(".preheader{display:none}");
    expect(result.html).toContain('style="width:600px;background:#fff"');
    expect(result.html).toContain('style="font-size:16px"');
  });

  it("keeps cid resources available for the future message protocol", () => {
    const result = sanitizeMessageHtml('<img src="cid:logo@example.org" alt="Logo" />');

    expect(result.html).toContain('src="cid:logo@example.org"');
    expect(result.blockedResources).toBe(0);
  });

  it("allows only explicitly permitted origins", () => {
    const result = sanitizeMessageHtml(
      '<img src="https://images.example/photo.png" />'
        + '<img src="https://tracking.example/pixel" />'
        + "<script>alert(1)</script>",
      { allowedOrigins: new Set(["https://images.example"]) },
    );

    expect(result.html).toContain('src="https://images.example/photo.png"');
    expect(result.html).not.toContain("tracking.example/pixel");
    expect(result.html).not.toContain("script");
    expect(result.blockedResources).toBe(1);
    expect(result.blockedOrigins).toEqual(["https://tracking.example"]);
  });

  it("allows all remote images only for an explicit one-message override", () => {
    const result = sanitizeMessageHtml(
      '<img src="https://images.example/photo.png" /><img src="http://cdn.example/pixel" />',
      { allowRemoteImages: true },
    );

    expect(result.html).toContain('src="https://images.example/photo.png"');
    expect(result.html).toContain('src="http://cdn.example/pixel"');
    expect(result.blockedResources).toBe(0);
  });
});
