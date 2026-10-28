import { sidebarFolderRows } from "./folderNav";
import { app } from "./stores.svelte";
import * as stores from "./stores.svelte";
import type { Mailbox } from "./types";

export type CommandGroup = "Triage" | "Compose" | "Folders" | "Help";

export type KeyChord = {
  key: string;
  shift?: boolean;
  ctrlOrMeta?: boolean;
};

export type Command = {
  id: string;
  title: string;
  group: CommandGroup;
  keys: KeyChord[];
  palette: boolean;
  when: () => boolean;
  run: () => void | Promise<void>;
};

const LETTER_BLOCK =
  "input, textarea, select, [contenteditable], [role='dialog'], [role='menu'], [popover], [aria-haspopup='listbox']";

function hasSelection(): boolean {
  return app.value.selectedMessageIds.length > 0;
}

function selectedFolder() {
  return app.value.folders.find((folder) => folder.id === app.value.selectedFolderId) ?? null;
}

function searching(): boolean {
  return app.value.searchQuery.trim().length > 0;
}

function loadedMessageMatches(): boolean {
  const message = app.value.message;
  if (!message) return false;
  return app.value.selectedMessageIds.includes(message.envelope.id);
}

function paletteHostAllowed(): boolean {
  return app.value.activeComposerId === null
    && !app.value.preferencesOpen
    && app.value.outboxAccountId === null;
}

function selectedEnvelopes() {
  const selected = new Set(app.value.selectedMessageIds);
  return app.value.envelopes.filter((envelope) => selected.has(envelope.id));
}

export function matchChord(
  event: Pick<KeyboardEvent, "key" | "shiftKey" | "ctrlKey" | "metaKey" | "altKey">,
  chord: KeyChord,
): boolean {
  const letter = chord.key.length === 1 && /[a-z]/i.test(chord.key);
  const keyOk = letter
    ? event.key.toLowerCase() === chord.key.toLowerCase()
    : event.key === chord.key;
  if (!keyOk) return false;
  if (event.altKey) return false;
  if (chord.ctrlOrMeta) {
    return (event.ctrlKey || event.metaKey) && !(event.ctrlKey && event.metaKey) && !event.shiftKey;
  }
  if (event.ctrlKey || event.metaKey) return false;
  const shiftedChar = chord.key === "#" || chord.key === "!" || chord.key === "?";
  if (shiftedChar) return true;
  if (chord.shift) return event.shiftKey;
  return !event.shiftKey;
}

function blockedLetter(target: Element | null): boolean {
  if (app.value.activeComposerId !== null || app.value.preferencesOpen || app.value.outboxAccountId) {
    return true;
  }
  return Boolean(target?.closest(LETTER_BLOCK));
}

function blockedPaletteChord(target: Element | null): boolean {
  if (!paletteHostAllowed()) return true;
  const dialog = target?.closest("[role='dialog']");
  return Boolean(dialog && dialog.getAttribute("aria-label") !== "Command palette");
}

function focusAfterTriage(event: KeyboardEvent) {
  const target = event.target as Element | null;
  const fromList = target?.closest?.("[data-navigation='message-list']");
  if (!fromList) return;
  const id = app.value.selectedEnvelope?.id;
  if (!id) return;
  requestAnimationFrame(() => {
    document.querySelector<HTMLElement>(`[data-item-id="${CSS.escape(id)}"]`)?.focus();
  });
}

function togglePalette() {
  app.value.commandPaletteOpen = !app.value.commandPaletteOpen;
}

export const catalog: Command[] = [
  {
    id: "delete-permanent",
    title: "Delete permanently",
    group: "Triage",
    keys: [{ key: "Backspace" }, { key: "Delete" }, { key: "#" }],
    palette: true,
    when: () => hasSelection() && selectedFolder()?.role === "Trash" && !searching(),
    run: () => void stores.deleteSelectedPermanently(),
  },
  {
    id: "trash",
    title: "Trash",
    group: "Triage",
    keys: [{ key: "Backspace" }, { key: "Delete" }, { key: "#" }],
    palette: true,
    when: () => hasSelection() && (selectedFolder()?.role !== "Trash" || searching()),
    run: () => void stores.moveSelectedToRole("Trash"),
  },
  {
    id: "archive",
    title: "Archive",
    group: "Triage",
    keys: [{ key: "e" }],
    palette: true,
    when: () => hasSelection(),
    run: () => void stores.moveSelectedToRole("Archive"),
  },
  {
    id: "junk",
    title: "Junk",
    group: "Triage",
    keys: [{ key: "!" }],
    palette: true,
    when: () => hasSelection(),
    run: () => void stores.moveSelectedToRole("Junk"),
  },
  {
    id: "star",
    title: "Star / Unstar",
    group: "Triage",
    keys: [{ key: "s" }],
    palette: true,
    when: () => hasSelection(),
    run: () => {
      const rows = selectedEnvelopes();
      const allFlagged = rows.length > 0 && rows.every((envelope) => envelope.flags.includes("Flagged"));
      void stores.setSelectedFlag("Flagged", !allFlagged);
    },
  },
  {
    id: "mark-unread",
    title: "Mark unread",
    group: "Triage",
    keys: [{ key: "u", shift: true }],
    palette: true,
    when: () => hasSelection(),
    run: () => void stores.setSelectedFlag("Seen", false),
  },
  {
    id: "mark-read",
    title: "Mark read",
    group: "Triage",
    keys: [{ key: "i", shift: true }],
    palette: true,
    when: () => hasSelection(),
    run: () => void stores.setSelectedFlag("Seen", true),
  },
  {
    id: "undo",
    title: "Undo",
    group: "Triage",
    keys: [{ key: "z" }],
    palette: true,
    when: () => app.value.undoAction !== null,
    run: () => stores.undoLastMessageAction(),
  },
  {
    id: "reply",
    title: "Reply",
    group: "Compose",
    keys: [{ key: "r" }],
    palette: true,
    when: () => loadedMessageMatches(),
    run: () => stores.openReplyComposer("reply"),
  },
  {
    id: "reply-all",
    title: "Reply all",
    group: "Compose",
    keys: [{ key: "a" }],
    palette: true,
    when: () => loadedMessageMatches(),
    run: () => stores.openReplyComposer("replyAll"),
  },
  {
    id: "forward",
    title: "Forward",
    group: "Compose",
    keys: [{ key: "f" }],
    palette: true,
    when: () => loadedMessageMatches(),
    run: () => void stores.forwardMessage(),
  },
  {
    id: "compose",
    title: "Compose new message",
    group: "Compose",
    keys: [{ key: "n" }],
    palette: true,
    when: () => app.value.activeComposerId === null,
    run: () => void stores.openComposer(),
  },
  {
    id: "go-unified",
    title: "Go to Unified Inbox",
    group: "Folders",
    keys: [],
    palette: true,
    when: () => true,
    run: () => void stores.selectUnifiedInbox(),
  },
  {
    id: "open-palette",
    title: "Open command palette",
    group: "Folders",
    keys: [{ key: "k", ctrlOrMeta: true }],
    palette: false,
    when: () => paletteHostAllowed(),
    run: togglePalette,
  },
  {
    id: "shortcuts",
    title: "Keyboard shortcuts",
    group: "Help",
    keys: [{ key: "?" }],
    palette: true,
    when: () => paletteHostAllowed(),
    run: () => {
      app.value.shortcutsOpen = true;
    },
  },
];

export function dispatchCommand(event: KeyboardEvent): boolean {
  if (event.repeat || event.defaultPrevented) return false;
  const target = event.target as Element | null;
  for (const command of catalog) {
    if (!command.keys.some((chord) => matchChord(event, chord))) continue;
    const ctrl = command.keys.some((chord) => chord.ctrlOrMeta);
    if (!ctrl && blockedLetter(target)) return false;
    if (ctrl && blockedPaletteChord(target)) return false;
    if (!command.when()) continue;
    event.preventDefault();
    void command.run();
    if (["archive", "trash", "delete-permanent", "junk"].includes(command.id)) {
      focusAfterTriage(event);
    }
    return true;
  }
  return false;
}

export function formatChord(chord: KeyChord): string {
  if (chord.ctrlOrMeta) return `Ctrl+${chord.key.toUpperCase()}`;
  if (chord.shift) return `Shift+${chord.key.toUpperCase()}`;
  if (chord.key === "Backspace") return "⌫";
  if (chord.key === "Delete") return "Del";
  if (chord.key.length === 1) return chord.key.toUpperCase();
  return chord.key;
}

export function commandChordLabel(id: string): string | null {
  const command = catalog.find((entry) => entry.id === id);
  const chord = command?.keys[0];
  return chord ? formatChord(chord) : null;
}

export type PaletteItem = {
  id: string;
  title: string;
  group: CommandGroup;
  chord: string | null;
  enabled: boolean;
  run: () => void | Promise<void>;
};

function score(query: string, text: string): number {
  const q = query.trim().toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 1;
  let ti = 0;
  let consecutive = 0;
  let points = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return 0;
    if (found === ti) consecutive += 1;
    else consecutive = 1;
    points += consecutive * 2;
    if (found === 0 || /\s/.test(t[found - 1]!)) points += 8;
    ti = found + 1;
  }
  return points;
}

function folderForMailbox(mailboxId: string): Mailbox | undefined {
  return app.value.folders.find(
    (folder) => folder.id === mailboxId || folder.sourceIds.includes(mailboxId),
  );
}

function selectionAccountIds(): string[] {
  const ids = new Set<string>();
  for (const envelope of selectedEnvelopes()) {
    const folders = [
      folderForMailbox(envelope.mailboxId),
      ...(envelope.sources ?? []).map((source) => folderForMailbox(source.mailboxId)),
    ];
    for (const folder of folders) {
      if (folder) ids.add(folder.accountId);
    }
  }
  return [...ids];
}

function livesInDestination(destination: Mailbox): boolean {
  const dest = new Set([destination.id, ...destination.sourceIds]);
  const targets = selectedEnvelopes();
  if (targets.length === 0) return false;
  return targets.every((target) => {
    const sources = target.sources?.length
      ? target.sources
      : [{ mailboxId: target.mailboxId }];
    return sources.every((source) => dest.has(source.mailboxId));
  });
}

function catalogItem(command: Command): PaletteItem {
  return {
    id: command.id,
    title: command.title,
    group: command.group,
    chord: commandChordLabel(command.id),
    enabled: command.when(),
    run: command.run,
  };
}

function folderItems(): PaletteItem[] {
  const accounts = new Map(app.value.accounts.map((account) => [account.dbId, account.name]));
  const accountIds = selectionAccountIds();
  const uniqueAccount = accountIds.length === 1 ? accountIds[0] : null;
  const items: PaletteItem[] = [];
  for (const row of sidebarFolderRows(app.value.folders)) {
    const folder = row.folder;
    if (!folder) continue;
    const accountName = accounts.get(folder.accountId) ?? folder.accountId;
    items.push({
      id: `go-folder:${folder.id}`,
      title: `Go to ${accountName} / ${row.label}`,
      group: "Folders",
      chord: null,
      enabled: true,
      run: () => void stores.selectFolder(folder.id),
    });
    if (uniqueAccount === folder.accountId && !livesInDestination(folder)) {
      items.push({
        id: `move-folder:${folder.id}`,
        title: `Move to ${accountName} / ${row.label}`,
        group: "Folders",
        chord: null,
        enabled: true,
        run: () => void stores.moveSelectedToFolder(folder.id),
      });
    }
  }
  return items;
}

export function paletteItems(query: string): PaletteItem[] {
  const trimmed = query.trim();
  const listed = catalog.filter((command) => command.palette).map(catalogItem);
  if (!trimmed) return listed;

  const scored = [
    ...listed.filter((item) => item.enabled).map((item) => ({
      item,
      points: score(trimmed, `${item.title} ${item.group}`),
    })),
    ...folderItems().map((item) => ({
      item,
      points: score(trimmed, `${item.title} ${item.group}`),
    })),
  ].filter((entry) => entry.points > 0);

  scored.sort((a, b) => b.points - a.points || a.item.title.localeCompare(b.item.title));
  return scored.slice(0, 40).map((entry) => entry.item);
}
