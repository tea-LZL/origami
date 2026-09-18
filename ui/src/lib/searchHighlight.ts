export interface HighlightPart {
  text: string;
  match: boolean;
}

const NON_TEXT_FILTERS = new Set(["read", "unread", "starred", "attachment"]);

export function highlightSearchText(value: string, query: string): HighlightPart[] {
  const terms = query
    .trim()
    .split(/\s+/)
    .map((token) => token.includes(":") ? token.slice(token.indexOf(":") + 1) : token)
    .map((term) => term.replace(/^"|"$/g, ""))
    .filter((term) => term.length > 1 && !NON_TEXT_FILTERS.has(term.toLowerCase()));
  if (terms.length === 0 || value.length === 0) return [{ text: value, match: false }];

  const haystack = value.toLocaleLowerCase();
  const ranges: [number, number][] = [];
  for (const term of terms) {
    const needle = term.toLocaleLowerCase();
    let start = haystack.indexOf(needle);
    while (start >= 0) {
      ranges.push([start, start + term.length]);
      start = haystack.indexOf(needle, start + term.length);
    }
  }
  if (ranges.length === 0) return [{ text: value, match: false }];

  ranges.sort((left, right) => left[0] - right[0] || right[1] - left[1]);
  const merged: [number, number][] = [];
  for (const [start, end] of ranges) {
    const previous = merged.length > 0 ? merged[merged.length - 1] : undefined;
    if (previous && start <= previous[1]) previous[1] = Math.max(previous[1], end);
    else merged.push([start, end]);
  }

  const parts: HighlightPart[] = [];
  let cursor = 0;
  for (const [start, end] of merged) {
    if (start > cursor) parts.push({ text: value.slice(cursor, start), match: false });
    parts.push({ text: value.slice(start, end), match: true });
    cursor = end;
  }
  if (cursor < value.length) parts.push({ text: value.slice(cursor), match: false });
  return parts;
}

export function withUnreadToken(query: string, unreadOnly: boolean): string {
  const tokens = query.trim().split(/\s+/).filter(Boolean);
  if (!unreadOnly) return tokens.join(" ");
  const hasReadState = tokens.some((token) => /^is:(un)?read$/i.test(token));
  if (hasReadState) return tokens.join(" ");
  return [...tokens, "is:unread"].join(" ");
}
