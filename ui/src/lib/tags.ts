const TAG_PALETTE = [
  "var(--tag-blue)", "var(--tag-violet)", "var(--tag-teal)", "var(--tag-green)",
  "var(--tag-amber)", "var(--tag-rose)",
];

export function tagColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = ((hash << 5) - hash + name.charCodeAt(i)) | 0;
  }
  return TAG_PALETTE[Math.abs(hash) % TAG_PALETTE.length];
}
