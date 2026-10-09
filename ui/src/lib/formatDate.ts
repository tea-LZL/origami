const MONTHS: Record<string, number> = {
  jan: 0, feb: 1, mar: 2, apr: 3, may: 4, jun: 5,
  jul: 6, aug: 7, sep: 8, oct: 9, nov: 10, dec: 11,
};

const MAIL_DATE = /^(?:[A-Za-z]{3},\s*)?(\d{1,2})\s+([A-Za-z]{3})\s+(\d{4})\s+(\d{2}):(\d{2})(?::(\d{2}))?\s+([+-])(\d{2})(\d{2})/;

/** Parse an RFC 5322 Date header into an absolute instant. */
export function parseMailDate(value: string): Date | null {
  const cleaned = value
    .replace(/\s*\([^)]*\)\s*$/, "")
    .replace(/\s+(UTC|UT|GMT)\s*$/i, " +0000")
    .trim();
  const match = MAIL_DATE.exec(cleaned);
  if (match) {
    const month = MONTHS[match[2].toLowerCase()];
    if (month !== undefined) {
      const day = Number(match[1]);
      const year = Number(match[3]);
      const hour = Number(match[4]);
      const minute = Number(match[5]);
      const second = Number(match[6] ?? "0");
      const sign = match[7] === "-" ? -1 : 1;
      const offsetMinutes = sign * (Number(match[8]) * 60 + Number(match[9]));
      const utc = Date.UTC(year, month, day, hour, minute, second) - offsetMinutes * 60_000;
      const date = new Date(utc);
      if (!Number.isNaN(date.getTime())) return date;
    }
  }
  const millis = Date.parse(cleaned);
  if (Number.isNaN(millis)) return null;
  const date = new Date(millis);
  return Number.isNaN(date.getTime()) ? null : date;
}

/**
 * Always render a mail timestamp in the runtime locale and local timezone.
 * Unparseable values are returned unchanged so a bad header is still visible.
 */
export function formatMessageDate(value: string | null | undefined): string {
  if (!value) return "";
  const date = parseMailDate(value);
  if (!date) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
