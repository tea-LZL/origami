/** Ctrl+N opens a new message. Bare N stays a separate, focus-sensitive binding. */
export function isNewMessageShortcut(event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">): boolean {
  return event.ctrlKey
    && !event.metaKey
    && !event.altKey
    && !event.shiftKey
    && event.key.toLowerCase() === "n";
}
