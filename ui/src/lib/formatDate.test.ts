import { describe, expect, it } from "vitest";
import { formatMessageDate, parseMailDate } from "./formatDate";

const local = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

describe("formatMessageDate", () => {
  it("renders the mail date in the local locale and timezone", () => {
    const raw = "Wed, 23 Sep 2026 17:48:19 +0100";
    const parsed = parseMailDate(raw);
    expect(parsed?.toISOString()).toBe("2026-09-23T16:48:19.000Z");
    expect(formatMessageDate(raw)).toBe(local.format(parsed!));
    expect(formatMessageDate(raw)).not.toContain("+0100");
  });

  it("ignores a parenthetical zone and a missing weekday", () => {
    expect(parseMailDate("Wed, 23 Sep 2026 19:08:44 +0800 (CST)")?.toISOString())
      .toBe("2026-09-23T11:08:44.000Z");
    expect(formatMessageDate("20 Sep 2026 20:01:00 +0200"))
      .toBe(local.format(new Date("2026-09-20T18:01:00.000Z")));
    expect(formatMessageDate("Fri, 18 Sep 2026 09:52:41 +0000 (UTC)"))
      .not.toMatch(/UTC|\+0000/);
    expect(formatMessageDate("Fri, 18 Sep 2026 09:52:41 UTC"))
      .toBe(local.format(new Date("2026-09-18T09:52:41.000Z")));
  });

  it("keeps an unparseable header instead of blanking it", () => {
    expect(formatMessageDate(null)).toBe("");
    expect(formatMessageDate("not a date")).toBe("not a date");
  });
});
