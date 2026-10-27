# Keyboard Triage and Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drive archive/trash/star/reply/undo from letter keys and a compact Ctrl+K palette, and auto-advance the list after destructive actions so the reading pane does not go empty.

**Architecture:** One `commands.ts` catalog is the source of truth. `dispatchCommand` runs from the window and the mail iframe. The palette is a Preferences-style overlay over `paletteItems(query)`. Auto-advance lives in `scheduleMessageAction` so keys, palette Move to, and the existing toolbar share it. No new Tauri commands.

**Tech Stack:** Svelte 5 runes, TypeScript, Vitest, Testing Library. UI-only.

**Spec:** `docs/superpowers/specs/2026-10-27-keyboard-triage-command-palette-design.md` — read it with this plan.

## Global Constraints

- No new npm/Cargo dependencies.
- UI stays provider-neutral; folder rows use `sidebarFolderRows()` / `FolderRow.label`, never IMAP `Mailbox.name`.
- One runnable Vitest regression per behavior; no Playwright.
- Prefer the store (`scheduleMessageAction`) over Svelte-side next-row reconstruction.
- `j`/`k` pane scoping stays: list moves rows, reading pane / iframe scrolls the body.
- `n`, Space, Ctrl+A, Alt+1/2/3, Ctrl+N, Ctrl+/−/0 stay outside the catalog.
- Preserve unrelated dirty-worktree files.
- **Do not commit unless the user explicitly asks.** Each task's checkpoint step is a no-op unless told otherwise.

## File Structure

| File | Responsibility |
|---|---|
| `ui/src/lib/folderNav.ts` | `sidebarFolderRows()` — labelled, fully expanded sidebar rows |
| `ui/src/lib/commands.ts` | Catalog, chord match, `dispatchCommand`, `paletteItems`, chord labels |
| `ui/src/lib/CommandPalette.svelte` | Compact 520px overlay; runs palette items |
| `ui/src/lib/stores.svelte.ts` | `commandPaletteOpen`; auto-advance; undo bumps `envelopeRequest`; unread cancels `markReadTimer` |
| `ui/src/App.svelte` | Window dispatch + Escape order + mount palette |
| `ui/src/lib/MessageView.svelte` | Iframe dispatch; toolbar titles; Trash-folder delete |
| `ui/src/lib/ThreadList.svelte` | Bulk-bar titles from catalog chords |

## Review Focus

Inputs the spec implies that a happy-path pass will miss:

1. **Ctrl+A vs `a`:** Ctrl+A must not fire Reply all.
2. **Ctrl+K vs dialogs:** Ctrl+K closes the palette (the palette is itself `role=dialog`) and is skipped for Preferences / Outbox / add-account / composer; it **does** run from the thread-list search input.
3. **`selectedEnvelope === null` with selected ids:** Ctrl+A / `clearMessageView` then Archive must walk / empty / load-more and not throw.
4. **Undo vs `loadMoreEnvelopes`:** `z` increments `envelopeRequest` and clears `envelopesLoadingMore`; an in-flight page-2 must not append onto the restored list.
5. **Shift+U vs delayed Seen:** explicit unread clears `markReadTimer` while the same row stays selected.

---

### Task 1: `sidebarFolderRows`

**Files:**
- Modify: `ui/src/lib/folderNav.ts` (export `sidebarFolderRows` next to `sidebarFolders`; keep `allExpandablePaths` available to it)
- Test: `ui/src/lib/folderNav.test.ts`

**Interfaces:**
- Consumes: `sidebarTree`, `flattenFolderTree`, `allExpandablePaths`, `FolderRow`.
- Produces: `sidebarFolderRows(folders: Mailbox[]): FolderRow[]` — all expandable paths expanded, rows with `folder == null` dropped.

- [ ] **Step 1: Write the failing test** in `folderNav.test.ts`

```ts
import { sidebarFolderRows } from "./folderNav";

describe("sidebarFolderRows", () => {
  it("uses_sidebar_labels_not_imap_names_and_hides_gmail_system_mailboxes", () => {
    const folders = [
      folder({ id: "all", name: "[Gmail]/All Mail", role: "Other" }),
      folder({ id: "inbox", name: "INBOX", role: "Inbox" }),
      folder({ id: "work", name: "Work", role: "Other" }),
      folder({ id: "acme", name: "Work/Acme", role: "Other" }),
    ];
    const rows = sidebarFolderRows(folders);
    expect(rows.map((row) => ({ id: row.id, label: row.label }))).toEqual([
      { id: "inbox", label: "Inbox" },
      { id: "work", label: "Work" },
      { id: "acme", label: "Acme" },
    ]);
    expect(rows.every((row) => row.folder != null)).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix ui test -- src/lib/folderNav.test.ts`
Expected: FAIL — `sidebarFolderRows` is not exported.

- [ ] **Step 3: Implement**

```ts
export function sidebarFolderRows(folders: Mailbox[]): FolderRow[] {
  const tree = sidebarTree(folders);
  return flattenFolderTree(tree, allExpandablePaths(tree))
    .filter((row) => row.folder != null);
}
```

Leave `allExpandablePaths` as a file-private function; `sidebarFolderRows` in the same file can call it. Do not change `sidebarFolders`.

- [ ] **Step 4: Re-run the test**

Run: `npm --prefix ui test -- src/lib/folderNav.test.ts`
Expected: PASS, including existing `sidebarFolders` cases.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 2: Catalog, chords, `dispatchCommand`

**Files:**
- Create: `ui/src/lib/commands.ts`
- Create: `ui/src/lib/commands.test.ts`
- Modify: `ui/src/lib/stores.svelte.ts` — add `commandPaletteOpen: boolean` to `State` / `initial` (default `false`)

**Interfaces:**
- Consumes: `app`, `moveSelectedToRole`, `deleteSelectedPermanently`, `setSelectedFlag`, `openReplyComposer`, `forwardMessage`, `undoLastMessageAction`, `openComposer`, `selectFolder`, `selectUnifiedInbox`, `moveSelectedToFolder` from `stores.svelte.ts`.
- Produces:

```ts
export type CommandGroup = "Triage" | "Compose" | "Folders";
export type KeyChord = { key: string; shift?: boolean; ctrlOrMeta?: boolean };
export type Command = {
  id: string;
  title: string;
  group: CommandGroup;
  keys: KeyChord[];
  palette: boolean;
  when: () => boolean;
  run: () => void | Promise<void>;
};
export const catalog: Command[];
export function matchChord(event: Pick<KeyboardEvent, "key" | "shiftKey" | "ctrlKey" | "metaKey" | "altKey">, chord: KeyChord): boolean;
export function dispatchCommand(event: KeyboardEvent): boolean;
export function commandChordLabel(id: string): string | null;
export function formatChord(chord: KeyChord): string;
```

`paletteItems` is Task 3 — do not add it yet. Tests that need a loaded message set `app.value.message` with `envelope.id` in `selectedMessageIds`.

- [ ] **Step 1: Write failing tests** in `ui/src/lib/commands.test.ts`

Stub `localStorage` like `stores.markread.test.ts`. Reset `app.value` fields used by `when()` in `beforeEach` (selection empty, no composer, no dialogs, `commandPaletteOpen: false`).

Helper to fire a key against a target:

```ts
function keyEvent(
  init: KeyboardEventInit,
  target: Element = document.body,
): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  Object.defineProperty(event, "target", { value: target });
  return event;
}

function resetMail() {
  app.value.selectedMessageIds = ["m1"];
  app.value.selectedEnvelope = { id: "m1" } as Envelope;
  app.value.selectedFolderId = "inbox";
  app.value.folders = [{
    id: "inbox", accountId: "db-1", name: "INBOX", role: "Inbox",
    subscribed: true, total: 1, unread: 1, sourceIds: [],
  }];
  app.value.searchQuery = "";
  app.value.message = null;
  app.value.activeComposerId = null;
  app.value.preferencesOpen = false;
  app.value.outboxAccountId = null;
  app.value.commandPaletteOpen = false;
  app.value.undoAction = null;
}
```

Required cases (one `it` each):

- `e` with selection calls `moveSelectedToRole("Archive")` and returns true.
- `s` mixed flags → `setSelectedFlag("Flagged", true)`; all flagged → `false`.
- `#`, `Backspace`, `Delete` with Inbox → trash (`moveSelectedToRole("Trash")`).
- `Backspace` in Trash with empty search → `deleteSelectedPermanently`; in Trash with `searchQuery = "foo"` → trash.
- `!` → junk.
- `r`/`a`/`f` no-op (`false`) without loaded matching message; with `app.value.message.envelope.id === "m1"` they call `openReplyComposer("reply")` / `"replyAll"` / `forwardMessage`.
- `Shift+U` / `Shift+I` → `setSelectedFlag("Seen", false|true)`.
- `z` only when `undoAction` is set.
- `Ctrl+K` and `Meta+K` set `commandPaletteOpen` true; `Ctrl+Meta+K` does not.
- `?` opens the palette.
- `event.repeat: true` on `e` returns false.
- Ctrl+A does not run reply-all (`dispatchCommand` false, `openReplyComposer` not called).
- Letter `e` targeting an `<input>` returns false; `Ctrl+K` targeting that same `<input>` returns true.
- Letter `e` with `activeComposerId` set returns false; `Ctrl+K` with composer active returns false.
- `Ctrl+K` with `preferencesOpen` returns false.
- `commandChordLabel("archive")` is `"E"`; `"trash"` is `"⌫"`.

In `commands.ts`, import stores as a namespace so Vitest spies work:

```ts
import * as stores from "./stores.svelte";
import { app } from "./stores.svelte";
// run: () => void stores.moveSelectedToRole("Archive")
```

In tests: `import * as stores from "./stores.svelte"; vi.spyOn(stores, "moveSelectedToRole")`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui test -- src/lib/commands.test.ts`
Expected: FAIL — cannot resolve `./commands`.

- [ ] **Step 3: Implement `commands.ts` and `commandPaletteOpen`**

Add `commandPaletteOpen: false` to `State` and `initial` in `stores.svelte.ts`.

Chord matcher:

```ts
export function matchChord(event: Pick<KeyboardEvent, "key" | "shiftKey" | "ctrlKey" | "metaKey" | "altKey">, chord: KeyChord): boolean {
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
```

Protected targets for **non-ctrlOrMeta** chords:

```ts
const LETTER_BLOCK = "input, textarea, select, [contenteditable], [role='dialog'], [role='menu'], [popover], [aria-haspopup='listbox']";
```

Also block when `activeComposerId`, `preferencesOpen`, or `outboxAccountId`.

**Ctrl+K exception (spec toggle vs dialog):** skip when `activeComposerId`, `preferencesOpen`, or `outboxAccountId`. Skip when `event.target.closest("[role=dialog]")` exists **and** its `aria-label` is not `"Command palette"`. Do **not** skip a thread-list `<input>` (no dialog). This is how Ctrl+K both opens from search and closes the palette.

`dispatchCommand`:

```ts
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

function focusAfterTriage(event: KeyboardEvent) {
  const target = event.target as Element | null;
  const fromList = target?.closest?.("[data-navigation='message-list']");
  if (!fromList) return; // iframe / reading pane: leave focus
  const id = app.value.selectedEnvelope?.id;
  if (!id) return;
  requestAnimationFrame(() => {
    document.querySelector<HTMLElement>(`[data-item-id="${CSS.escape(id)}"]`)?.focus();
  });
}
```

Catalog **file order** (delete-permanent before trash):

| id | title | group | keys | palette | when / run |
|---|---|---|---|---|---|
| `delete-permanent` | Delete permanently | Triage | Backspace, Delete, `#` | true | selection && folder.role === `"Trash"` && `!searchQuery.trim()` → `deleteSelectedPermanently()` |
| `trash` | Trash | Triage | Backspace, Delete, `#` | true | selection && (role ≠ Trash \|\| search nonempty) → `moveSelectedToRole("Trash")` |
| `archive` | Archive | Triage | `e` | true | selection → `moveSelectedToRole("Archive")` |
| `junk` | Junk | Triage | `!` | true | selection → `moveSelectedToRole("Junk")` |
| `star` | Star / Unstar | Triage | `s` | true | selection; unstar iff every selected envelope has `Flagged` |
| `mark-unread` | Mark unread | Triage | `{key:"u", shift:true}` | true | selection → `setSelectedFlag("Seen", false)` |
| `mark-read` | Mark read | Triage | `{key:"i", shift:true}` | true | selection → `setSelectedFlag("Seen", true)` |
| `undo` | Undo | Triage | `z` | true | `undoAction != null` → `undoLastMessageAction()` |
| `reply` | Reply | Compose | `r` | true | loaded message id in selection → `openReplyComposer("reply")` |
| `reply-all` | Reply all | Compose | `a` | true | same → `"replyAll"` |
| `forward` | Forward | Compose | `f` | true | same → `forwardMessage()` |
| `compose` | Compose new message | Compose | `[]` | true | `activeComposerId == null` → `openComposer()` |
| `go-unified` | Go to Unified Inbox | Folders | `[]` | true | always → `selectUnifiedInbox()` |
| `open-palette` | Open command palette | Folders | `{key:"k", ctrlOrMeta:true}` | **false** | !composer && !prefs && !outbox → toggle `commandPaletteOpen` |
| `open-palette-help` | Open command palette | Folders | `?` | **false** | same as open-palette |

`when()` for the two open-palette commands must stay true while the palette is open so Ctrl+K toggles it closed.

`formatChord`: `ctrlOrMeta` → `Ctrl+K`; `shift` → `Shift+U`; `Backspace` → `⌫`; `Delete` → `Del`; length-1 → upper case.

`commandChordLabel(id)`: first chord of that command, or `null`.

Star `run`:

```ts
const selected = new Set(app.value.selectedMessageIds);
const rows = app.value.envelopes.filter((e) => selected.has(e.id));
const allFlagged = rows.length > 0 && rows.every((e) => e.flags.includes("Flagged"));
void setSelectedFlag("Flagged", !allFlagged);
```

- [ ] **Step 4: Re-run tests**

Run: `npm --prefix ui test -- src/lib/commands.test.ts`
Expected: PASS.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 3: `paletteItems` fuzzy list

**Files:**
- Modify: `ui/src/lib/commands.ts`
- Modify: `ui/src/lib/commands.test.ts`

**Interfaces:**
- Consumes: `catalog` (Task 2), `sidebarFolderRows` (Task 1), `app.value.accounts` (`dbId` ↔ `folder.accountId`), `app.value.folders`, selection.
- Produces:

```ts
export type PaletteItem = {
  id: string;
  title: string;
  group: CommandGroup;
  chord: string | null;
  enabled: boolean;
  run: () => void | Promise<void>;
};
export function paletteItems(query: string): PaletteItem[];
```

Dynamic ids: `go-folder:${folder.id}`, `move-folder:${folder.id}`.

- [ ] **Step 1: Write failing tests** in `commands.test.ts`

Use two accounts (`Work` dbId `db-1`, `Home` dbId `db-2`), folders Inbox/Archive on db-1 (Inbox name `INBOX`, role Inbox), a Gmail All Mail Other folder, and a Home Inbox.

Empty query:

- includes Archive, Trash, Compose new message, Go to Unified Inbox
- does **not** include `open-palette` / `Open command palette`
- does **not** include any `Go to` / `Move to` folder row
- disabled rows still present when `selectedMessageIds` is empty (Archive `enabled: false`)

Query `"inb"` with a db-1 selection:

- includes `Go to Work / Inbox` (label Inbox, not INBOX)
- includes `Move to Work / Archive`
- does **not** include `Move to Work / Inbox` (already there)
- does **not** include `Move to Home / Inbox`
- does **not** include All Mail
- does **not** include disabled catalog rows

Query `"inb"` with mixed-account selected envelopes: no `Move to` rows.

Query `"zzzz"`: `[]`.

Forty-plus folders: result length ≤ 40.

Scoring: query `"arc"` ranks `Archive` (title word-start) above a folder named `Research archive` if both match — assert Archive’s index is lower.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui test -- src/lib/commands.test.ts`
Expected: FAIL — `paletteItems` is not exported.

- [ ] **Step 3: Implement `paletteItems`**

Empty query: `catalog.filter(c => c.palette)` mapped to `PaletteItem` with `enabled: c.when()`, `chord: commandChordLabel(c.id)`. Group order is catalog order (Triage, Compose, Folders already).

Non-empty query:

1. Enabled catalog rows with `palette: true` whose `score(query, title+" "+group) > 0`.
2. For each `sidebarFolderRows(app.value.folders)`: `Go to {account.name} / {row.label}` always; `Move to` only if selection nonempty, every selected envelope’s folder `accountId` is the same as `row.folder.accountId`, and the selection does not already live in that folder (mirror `moveSelectedToFolder`: skip when every source mailbox is the destination or in `destination.sourceIds`).
3. Filter score > 0, sort by score desc then title, `slice(0, 40)`.

Subsequence score (case-insensitive):

```ts
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
    if (found === ti) consecutive += 1; else consecutive = 1;
    points += consecutive * 2;
    if (found === 0 || /\s/.test(t[found - 1]!)) points += 8;
    ti = found + 1;
  }
  return points;
}
```

`run` for go-folder: `selectFolder(id)` then `app.value.commandPaletteOpen = false` is the palette’s job (Task 5). Catalog `run` functions do **not** close the palette.

- [ ] **Step 4: Re-run tests**

Run: `npm --prefix ui test -- src/lib/commands.test.ts`
Expected: PASS.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 4: Auto-advance, undo generation, unread timer

**Files:**
- Modify: `ui/src/lib/stores.svelte.ts` — `scheduleMessageAction`, `removeSelectedLocally` call site, `undoLastMessageAction`, `setSelectedFlag`
- Create: `ui/src/lib/stores.advance.test.ts`
- Modify: `ui/src/lib/stores.markread.test.ts`

**Interfaces:**
- Consumes: existing `scheduleMessageAction`, `selectEnvelopeExclusive`, `loadMoreEnvelopes`, `retainForUnreadFilter`.
- Produces: same public functions; behavior change only. Do not export `envelopeRequest`.

- [ ] **Step 1: Write failing tests**

`stores.advance.test.ts` — same api mock + `stubLocalStorage` + fake timers as `stores.markread.test.ts`. Also mock `api.moveMessages` and `api.listEnvelopes`. Set `markReadDelay: -1` so auto-advance does not flip Seen. Provide Inbox + Archive folders on the same account. Envelopes need `sources: [{ mailboxId, serverUid }]`.

```ts
it("archive_selects_the_slot_next_remaining_row", async () => {
  app.value.envelopes = [env("a"), env("b"), env("c")];
  app.value.selectedEnvelope = env("a");
  app.value.selectedMessageIds = ["a"];
  app.value.hasMoreEnvelopes = false;
  await moveSelectedToRole("Archive");
  expect(app.value.envelopes.map((e) => e.id)).toEqual(["b", "c"]);
  expect(app.value.selectedEnvelope?.id).toBe("b");
  expect(app.value.selectedMessageIds).toEqual(["b"]);
});

it("archive_last_row_selects_previous", async () => { /* a,b selected b → a */ });

it("archive_last_remaining_row_empties_the_pane", async () => { /* one row → selectedEnvelope null, message null */ });

it("unread_only_does_not_keep_the_archived_id", async () => {
  app.value.unreadOnly = true;
  app.value.envelopes = [env("a"), env("b")]; // both unseen
  // archive a → page is [b], not [a]
});

it("keep_open_when_open_row_was_not_a_target", async () => {
  app.value.envelopes = [env("a"), env("b"), env("c")];
  app.value.selectedEnvelope = env("c");
  app.value.selectedMessageIds = ["a"];
  await moveSelectedToRole("Archive");
  expect(app.value.selectedEnvelope?.id).toBe("c");
  expect(app.value.selectedMessageIds).toEqual(["c"]);
});

it("null_open_row_with_selected_ids_does_not_throw", async () => {
  app.value.envelopes = [env("a"), env("b")];
  app.value.selectedEnvelope = null;
  app.value.selectedMessageIds = ["a", "b"];
  await expect(moveSelectedToRole("Archive")).resolves.toBeUndefined();
  expect(app.value.envelopes).toEqual([]);
});

it("undo_restores_envelopes_and_selection_and_drops_inflight_loadMore", async () => {
  app.value.envelopes = [env("only")];
  app.value.selectedEnvelope = env("only");
  app.value.selectedMessageIds = ["only"];
  app.value.hasMoreEnvelopes = true;
  app.value.selectedFolderId = "inbox";
  let resolvePage: (rows: Envelope[]) => void = () => {};
  vi.mocked(api.listEnvelopes).mockImplementation(
    () => new Promise((resolve) => { resolvePage = resolve; }),
  );
  await moveSelectedToRole("Archive");
  expect(app.value.envelopes).toEqual([]);
  undoLastMessageAction();
  expect(app.value.envelopes.map((e) => e.id)).toEqual(["only"]);
  expect(app.value.envelopesLoadingMore).toBe(false);
  resolvePage([env("page2")]);
  await Promise.resolve();
  await Promise.resolve();
  expect(app.value.envelopes.map((e) => e.id)).toEqual(["only"]);
  expect(app.value.selectedMessageIds).toEqual(["only"]);
});
```

Empty-page load-more happy path: `hasMoreEnvelopes true`, `listEnvelopes` resolves immediately to `[env("page2")]`, after archive of the last row `selectedEnvelope.id === "page2"` and `selectedMessageIds` was empty before that select.

`stores.markread.test.ts` add:

```ts
it("explicit_unread_cancels_pending_mark_read", async () => {
  app.value.markReadDelay = 3;
  await selectEnvelope(envelope("row-1"));
  await vi.advanceTimersByTimeAsync(0);
  await setSelectedFlag("Seen", false);
  await vi.advanceTimersByTimeAsync(5_000);
  expect(app.value.envelopes[0].flags).not.toContain("Seen");
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui test -- src/lib/stores.advance.test.ts src/lib/stores.markread.test.ts`
Expected: FAIL — archive still nulls `selectedEnvelope`; unread timer still fires.

- [ ] **Step 3: Implement**

In `scheduleMessageAction`, **before** `removeSelectedLocally`:

```ts
function nextAfterRemoval(envelopes: Envelope[], removed: Set<string>): Envelope | null {
  const first = envelopes.findIndex((e) => removed.has(e.id));
  if (first < 0) return envelopes.find((e) => !removed.has(e.id)) ?? null;
  for (let i = first; i < envelopes.length; i++) {
    if (!removed.has(envelopes[i]!.id)) return envelopes[i]!;
  }
  for (let i = first - 1; i >= 0; i--) {
    if (!removed.has(envelopes[i]!.id)) return envelopes[i]!;
  }
  return null;
}

const removed = new Set(targets.map((t) => t.id));
const open = app.value.selectedEnvelope;
const next = open != null && !removed.has(open.id)
  ? open
  : nextAfterRemoval(app.value.envelopes, removed);
removeSelectedLocally(targets);
if (next && app.value.envelopes.some((e) => e.id === next.id)) {
  void selectEnvelopeExclusive(next);
} else if (app.value.envelopes.length === 0 && app.value.hasMoreEnvelopes) {
  const request = envelopeRequest;
  void loadMoreEnvelopes().then(() => {
    if (request !== envelopeRequest) return;
    if (app.value.selectedMessageIds.length > 0) return;
    const first = app.value.envelopes[0];
    if (first) void selectEnvelopeExclusive(first);
  });
}
```

`undoLastMessageAction`:

```ts
export function undoLastMessageAction() {
  if (!pendingUndo) return;
  clearTimeout(pendingUndo.timer);
  envelopeRequest += 1;
  pendingUndo.restore();
  pendingUndo = null;
  patch({ undoAction: null, envelopesLoadingMore: false });
}
```

`setSelectedFlag`: when `flag === "Seen" && !enabled`, `clearTimeout(markReadTimer); markReadTimer = null;` before the optimistic patch.

Do not bump `envelopeRequest` on the 6s execute path.

- [ ] **Step 4: Re-run tests**

Run: `npm --prefix ui test -- src/lib/stores.advance.test.ts src/lib/stores.markread.test.ts`
Expected: PASS, including existing delayed-mark-read cases.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 5: `CommandPalette.svelte`

**Files:**
- Create: `ui/src/lib/CommandPalette.svelte`
- Create: `ui/src/lib/CommandPalette.test.ts`

**Interfaces:**
- Consumes: `paletteItems`, `formatChord` / item `chord`, `app.value.commandPaletteOpen`.
- Produces: overlay UI; running an item sets `commandPaletteOpen = false` then focuses the next list row per spec (override `trapFocus` restore).

- [ ] **Step 1: Write failing tests** in `CommandPalette.test.ts`

Mock `./commands` with a hoisted `paletteItems` that:

- empty query → two items: `{ id: "archive", title: "Archive", group: "Triage", chord: "E", enabled: true, run }` and a disabled `{ id: "undo", title: "Undo", ... enabled: false }`
- query `"inb"` → `{ id: "go-folder-inbox", title: "Go to Work / Inbox", group: "Folders", chord: null, enabled: true, run }`

Mock `./stores.svelte` `{ app: { value: { commandPaletteOpen: true, selectedEnvelope: { id: "next" } } } }`.

```ts
it("renders_dialog_and_closes_on_Escape_and_overlay_click", async () => { /* trapFocus + overlay */ });
it("empty_query_shows_grouped_cheat_sheet_with_chords", async () => {
  render(CommandPalette);
  expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  expect(screen.getByText("Triage")).toBeInTheDocument();
  expect(screen.getByRole("option", { name: /Archive/ })).toHaveTextContent("E");
});
it("typing_filters_and_Enter_runs_highlighted_and_closes", async () => {
  const run = vi.fn();
  // paletteItems mock returns the inbox row for "inb"
  render(CommandPalette);
  const box = screen.getByRole("combobox");
  await fireEvent.input(box, { target: { value: "inb" } });
  await fireEvent.keyDown(box, { key: "Enter" });
  expect(run).toHaveBeenCalledTimes(1);
  expect(mocks.app.value.commandPaletteOpen).toBe(false);
});
```

Disabled Undo is in the DOM on empty query and is not the initial highlight (Archive is).

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui test -- src/lib/CommandPalette.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the compact sheet**

Markup:

```svelte
{#if app.value.commandPaletteOpen}
  <div
    class="overlay"
    role="presentation"
    use:trapFocus={{ onEscape: close }}
    onclick={onOverlay}
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <input
        bind:this={inputEl}
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={activeId}
        aria-autocomplete="list"
        aria-label="Command"
        placeholder="Type a command or folder"
        bind:value={query}
        onkeydown={onSearchKey}
      />
      <div id={listId} role="listbox">
        <!-- grouped options; disabled get aria-disabled and are skipped by arrows -->
      </div>
      <p class="hint">Enter run · Esc close</p>
    </div>
  </div>
{/if}
```

Behavior:

- `query` state; `items = $derived(paletteItems(query))`.
- Initial / on items change: highlight index = first `enabled` item.
- Empty query shows group headers (Triage, Compose, Folders) when that group has rows.
- Overlay click: `if (event.target === event.currentTarget) close()`.
- `close()` sets `commandPaletteOpen = false`.
- `onSearchKey`: ArrowUp/Down move among **enabled** items; when `query === ""`, `j`/`k` do the same; Enter runs highlighted `run()` then `close()` then `requestAnimationFrame` focus:
  - if `app.value.selectedEnvelope?.id`, `[data-item-id=…]`
  - else `[data-navigation="message-list"]`
  Do this **after** close so `trapFocus` destroy runs first.
- No matches (`items.length === 0` && query nonempty): one non-option `<p>No matching commands</p>`.
- CSS: overlay `position:fixed; inset:0; z-index:80; display:grid; align-items:start; justify-items:center; padding-top:12vh; background: var(--overlay)`. Card `width: min(520px, calc(100vw - 32px)); max-height: min(480px, 70vh);` paper gradient / border / `var(--radius-lg)` / `var(--shadow-float)` matching Preferences. List `overflow-y: auto`. Highlight uses `var(--select-option-selected)`. `<kbd>` uses `var(--bg-sunken)`.

- [ ] **Step 4: Re-run tests**

Run: `npm --prefix ui test -- src/lib/CommandPalette.test.ts`
Expected: PASS.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 6: Wire `App.svelte`

**Files:**
- Modify: `ui/src/App.svelte` (`onKeydown` and mount)

**Interfaces:**
- Consumes: `dispatchCommand` (Task 2), `CommandPalette` (Task 5).
- Produces: window keys reach the catalog; Escape closes the palette first.

There is no App.svelte unit test today. Verify with the existing commands tests plus a manual read of the handler. Add a small `appShortcuts`-style test only if you extract the Escape order; otherwise keep the handler in App and assert via `dispatchCommand` tests already in Task 2.

- [ ] **Step 1: Change `onKeydown`**

```ts
import { dispatchCommand } from "./lib/commands";

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    if (app.value.commandPaletteOpen) {
      e.preventDefault();
      app.value.commandPaletteOpen = false;
      return;
    }
    if (app.value.preferencesOpen) {
      e.preventDefault();
      app.value.preferencesOpen = false;
      return;
    }
    if (app.value.outboxAccountId) {
      e.preventDefault();
      app.value.outboxAccountId = null;
      return;
    }
    if (app.value.activeComposerId !== null) {
      e.preventDefault();
      minimizeComposer();
    }
    return;
  }
  if (dispatchCommand(e)) return;
  // Keep the existing Alt+1/2/3 block and the ctrl/meta early-return + `n` compose
  // exactly as they are today, after this dispatchCommand call.
}
```

`dispatchCommand` must run **before** the `e.ctrlKey` early-return so Ctrl+K is visible.

- [ ] **Step 2: Mount the palette** next to Preferences / Outbox:

```svelte
<AddAccount bind:this={addAccount} />
<Preferences />
<Outbox />
<CommandPalette />
```

- [ ] **Step 3: Run UI tests that still parse App**

Run: `npm --prefix ui test -- src/lib/commands.test.ts src/lib/CommandPalette.test.ts`
Expected: PASS.

- [ ] **Step 4: Checkpoint** — do not commit unless explicitly asked.

---

### Task 7: MessageView iframe + toolbar, ThreadList titles

**Files:**
- Modify: `ui/src/lib/MessageView.svelte` (`bindEmailFrame`, toolbar)
- Modify: `ui/src/lib/MessageView.test.ts`
- Modify: `ui/src/lib/ThreadList.svelte` (bulk-bar `title`s)

**Interfaces:**
- Consumes: `dispatchCommand`, `commandChordLabel`, `deleteSelectedPermanently`.
- Produces: iframe keys run the catalog; titles like `Archive (E)`; Trash folder icon deletes permanently.

- [ ] **Step 1: Update MessageView tests first**

Extend the hoisted store mock with `selectedFolderId`, `folders`, `searchQuery`, `deleteSelectedPermanently`.

Mock `./commands` so MessageView does not load the real catalog (which imports every store export the MessageView mock does not provide):

```ts
vi.mock("./commands", () => ({
  dispatchCommand: vi.fn(() => false),
  commandChordLabel: (id: string) =>
    ({
      archive: "E",
      trash: "⌫",
      "delete-permanent": "⌫",
      junk: "!",
      star: "S",
      reply: "R",
      "reply-all": "A",
      forward: "F",
    }[id] ?? null),
}));
```

Change the icon tooltip assertion:

```ts
expect(screen.getByRole("button", { name: "Archive" })).toHaveAttribute("title", "Archive (E)");
expect(screen.getByRole("button", { name: "Trash" })).toHaveAttribute("title", "Trash (⌫)");
expect(screen.getByRole("button", { name: "Junk" })).toHaveAttribute("title", "Junk (!)");
expect(screen.getByRole("button", { name: "Star" })).toHaveAttribute("title", "Star (S)");
expect(screen.getByRole("button", { name: "Reply" })).toHaveAttribute("title", "Reply (R)");
expect(screen.getByRole("button", { name: "Reply all" })).toHaveAttribute("title", "Reply all (A)");
expect(screen.getByRole("button", { name: "Forward" })).toHaveAttribute("title", "Forward (F)");
```

Add:

```ts
it("trash_icon_in_trash_folder_deletes_permanently", async () => {
  mocks.app.value.selectedFolderId = "trash";
  mocks.app.value.searchQuery = "";
  mocks.app.value.folders = [{ id: "trash", role: "Trash", accountId: "db-1", name: "Trash", subscribed: true, total: 1, unread: 0, sourceIds: [] }];
  render(MessageView);
  const button = screen.getByRole("button", { name: "Delete permanently" });
  expect(button).toHaveAttribute("title", "Delete permanently (⌫)");
  await fireEvent.click(button);
  expect(mocks.deleteSelectedPermanently).toHaveBeenCalled();
  expect(mocks.moveSelectedToRole).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm --prefix ui test -- src/lib/MessageView.test.ts`
Expected: FAIL — titles still `"Archive"`; no Delete permanently button.

- [ ] **Step 3: Implement MessageView**

```ts
import { dispatchCommand, commandChordLabel } from "./commands";
import { deleteSelectedPermanently } from "./stores.svelte";

function titled(id: string, name: string): string {
  const chord = commandChordLabel(id);
  return chord ? `${name} (${chord})` : name;
}

const inTrashFolder = $derived(
  app.value.folders.find((f) => f.id === app.value.selectedFolderId)?.role === "Trash"
  && app.value.searchQuery.trim() === "",
);
```

Iframe keydown:

```ts
document.addEventListener("keydown", (keyEvent) => {
  if (dispatchCommand(keyEvent)) return;
  onBodyKeydown(keyEvent);
  if (bodyEl) handleMessageBodyKeydown(keyEvent, bodyEl);
});
```

Toolbar: Archive/Star/Reply/etc use `titled(...)`. Trash button:

```svelte
<button
  type="button"
  class="icon"
  aria-label={inTrashFolder ? "Delete permanently" : "Trash"}
  title={titled(inTrashFolder ? "delete-permanent" : "trash", inTrashFolder ? "Delete permanently" : "Trash")}
  onclick={() => inTrashFolder ? deleteSelectedPermanently() : moveSelectedToRole("Trash")}
>
```

ThreadList bulk bar: import `commandChordLabel` and set `title={commandChordLabel("archive") ? `Archive (${commandChordLabel("archive")})` : "Archive"}` on Archive/Trash/Junk/Star/Read (`mark-read`)/Unread (`mark-unread`). Trash in the bulk bar already has a separate Delete button when `role === "Trash"` — leave that split; only add titles. ThreadList tests mock `./stores.svelte`; add the same `./commands` mock as MessageView so the component can import `commandChordLabel`.

- [ ] **Step 4: Re-run MessageView tests plus the suite**

Run:

```
npm --prefix ui test -- --run
npm --prefix ui run check
```

Expected: all existing tests pass; 0 svelte-check issues.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

## Self-review (planner)

| Spec requirement | Task |
|---|---|
| `sidebarFolderRows` / sidebar labels | 1 |
| Catalog, chords, Ctrl+A, exemptions, Ctrl+K from search, iframe-ready `dispatchCommand` | 2 |
| List vs iframe focus after triage | 2 (`focusAfterTriage`) |
| `paletteItems` fuzzy, Go to / Move to, cap 40, no open-palette row | 3 |
| Auto-advance, unread-only, null open row, loadMore, undo `envelopeRequest`, unread timer | 4 |
| Compact palette UI, trapFocus, Enter, overlay, post-close focus | 5 |
| App window dispatch + Escape order | 6 |
| Iframe dispatch, titles, Trash-folder delete, bulk titles | 7 |
| Covering `npm test` + `svelte-check` | 7 |

No Playwright, no rebinding, no Gmail chords, no Tauri.
