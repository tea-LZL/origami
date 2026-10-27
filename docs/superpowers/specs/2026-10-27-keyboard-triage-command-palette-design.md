# Origami — Keyboard Triage and Command Palette Design

Date: 2026-10-27
Status: approved design, pending implementation plan
Scope: one command catalog driving letter-key triage, a compact Ctrl+K
palette, and auto-advance after destructive list actions.

## Goals

1. **Keyboard triage** — archive, trash, junk, star, reply, unread, and undo
   run from the list or the reading pane without the mouse.
2. **Command palette** — Ctrl+K / `?` opens a compact paper sheet that runs
   the same catalog and, once the query is non-empty, Go to / Move to folder.
3. **Keep reading** — archive / trash / junk / move / permanent delete select
   the next remaining row so the reading pane does not go empty.

## Non-goals

- User-rebinding or a Preferences keymap UI.
- Gmail chord sequences (`g i`, `g s`).
- Changing `j` / `k` pane scoping (list moves rows; reading pane / iframe
  scrolls the body). `n`, Space, Ctrl+A, Alt+1/2/3 stay outside the catalog.
- Searching message bodies in the palette (the thread-list search box already
  does that).
- Tags, saved searches, or Preferences entries in the palette.
- New Tauri commands, store schema, or IMAP behavior.
- Reopening server Drafts rows into a composer.

## Baseline (as-built facts)

- Window `keydown` in `ui/src/App.svelte` handles Escape (prefs, outbox,
  minimize composer), Alt+1/2/3, and `n` for compose. Ctrl-modified keys
  return early. `isNewMessageShortcut` is Ctrl+N (`ui/src/lib/appShortcuts.ts`).
- List `j`/`k`/arrows, Space, Shift-range, Ctrl+A, Escape-clear live in
  `ThreadList.svelte` against `visibleEnvelopes`. Body `j`/`k` scroll in
  `navigation.ts`. The mail iframe already forwards keydown for scroll and
  zoom (`MessageView.svelte` `bindEmailFrame`).
- Archive / Trash / Junk / Star / Reply / Forward are mouse toolbars calling
  `moveSelectedToRole`, `setSelectedFlag`, `openReplyComposer`,
  `forwardMessage`. Reply during load is a no-op (needs `app.value.message`).
- `selectEnvelopeExclusive` sets `selectedMessageIds` to `[id]`, so the open
  row is a valid triage target.
- Destructive moves go through `scheduleMessageAction` →
  `removeSelectedLocally` (clears selection, nulls `selectedEnvelope` /
  `message` if they were removed) plus a 6s undo toast
  (`undoLastMessageAction`). Mouse Archive today blanks the reading pane.
- Unread-only keep-selected-on-Seen lives in `ui/src/lib/unreadList.ts`.
- Delayed mark-as-read uses module-level `markReadTimer` in
  `stores.svelte.ts`; the timer callback still fires if the same envelope is
  selected.
- Dialogs use overlay + `trapFocus` (Preferences z-index 80, 440px paper
  card). TipTap Link uses Ctrl+K in the composer.
- Visible sidebar folders: `sidebarFolders` / `sidebarTree` in
  `folderNav.ts` (hides Other-role All Mail / Important / Starred).
- `selectFolder` / `selectUnifiedInbox` already switch the list. Move
  already refuses cross-account destinations.

---

## §1 Architecture

Three units, one table. No new backend.

**Catalog** (`ui/src/lib/commands.ts`) is the only place a command is named.
Each entry:

```
id: string
title: string
group: "Triage" | "Compose" | "Folders"
keys: KeyChord[]          // empty ⇒ palette-only
palette: boolean          // false ⇒ dispatch-only (open palette itself)
when(): boolean
run(): void | Promise<void>
```

`run()` calls existing store functions. `when()` reads `app.value`.

**Dispatcher** `dispatchCommand(event): boolean` is called from:

1. `App.svelte` window `keydown` (before the current Ctrl-early-return).
2. The sandboxed mail iframe document keydown in `MessageView.svelte`.

If it matches an enabled command it `preventDefault`s, runs `run()`, and
returns true. Matching walks the catalog in file order; the first entry
whose `keys` match and `when()` is true wins (`delete-permanent` must
precede `trash`). `j`/`k` are not in the catalog, so the iframe still
scrolls.

**Focus after auto-advance** (one rule, so `j`/`k` pane scoping stays):

- Event originated in the thread list → rAF-focus the next `[data-item-id]`
  (same as today’s `j`/`k`).
- Event originated in the reading pane or mail iframe → leave focus where
  it is; the new body replaces the old one; `j`/`k` still scroll.
- Palette run (does not go through `dispatchCommand`) → close first so
  `trapFocus` destroy runs, then **override** restored focus: if a next
  envelope exists, focus its list row; otherwise focus the listbox. Do not
  leave focus on a recycled virtual-list node for a removed id.
- Mouse toolbar → leave focus on the toolbar.

**Palette** (`ui/src/lib/CommandPalette.svelte`) is a Preferences-style
overlay. Open state is `app.value.commandPaletteOpen` (boolean, default
false), matching `preferencesOpen`. Catalog entries `open-palette` /
`open-palette-help` toggle that flag.

`j`/`k` stay pane-scoped. Destructive triage computes the next envelope
*inside* `scheduleMessageAction` so keyboard, palette Move to, and the
existing toolbar buttons share auto-advance.

---

## §2 Commands, keys, and focus

### Chord matching

`KeyChord` is `{ key: string; shift?: boolean; ctrlOrMeta?: boolean }`.
A keyboard event matches a chord iff:

1. `event.key` equals `key`, or `event.key.toLowerCase() === key.toLowerCase()`
   when `key` is a letter.
2. **Modifiers, dispatch-only letters / `Backspace` / `Delete`:** no
   `ctrlKey`, no `metaKey`, no `altKey`. `shift` is required when the chord
   sets `shift: true` (`Shift+U`, `Shift+I`) and forbidden for the bare
   letters `e s r a f z`.
3. **`#`, `!`, `?`:** match `event.key` exactly; `shiftKey` is ignored
   (layout-dependent). Still no ctrl/meta/alt.
4. **`Ctrl+K` / `Cmd+K`:** `ctrlOrMeta: true` ⇒ `(ctrlKey || metaKey) &&
   !(ctrlKey && metaKey)`, no alt, no shift, `key.toLowerCase() === "k"`.

Bare `a` therefore cannot collide with list Ctrl+A. `dispatchCommand` still
runs before App’s ctrl-early-return so rule 4 can see Ctrl+K.

### Key table

| Chord | `id` | Title | `when()` |
|---|---|---|---|
| `e` | `archive` | Archive | `selectedMessageIds.length > 0` |
| `Backspace`, `Delete`, `#` | `trash` | Trash | selection nonempty **and** (current folder role ≠ Trash **or** `searchQuery.trim()` is nonempty) |
| `Backspace`, `Delete`, `#` | `delete-permanent` | Delete permanently | selection nonempty **and** current folder role is Trash **and** `searchQuery.trim()` is empty |
| `!` | `junk` | Junk | selection nonempty |
| `s` | `star` | Star / Unstar | selection nonempty. If every selected row has `Flagged`, unstar; otherwise star |
| `r` | `reply` | Reply | `app.value.message` exists and its envelope id is in the selection |
| `a` | `reply-all` | Reply all | same as reply |
| `f` | `forward` | Forward | same as reply |
| `Shift+U` | `mark-unread` | Mark unread | selection nonempty |
| `Shift+I` | `mark-read` | Mark read | selection nonempty |
| `z` | `undo` | Undo | `app.value.undoAction !== null` |
| `Ctrl+K` or `Cmd+K` | `open-palette` | Open command palette | `palette: false`. No other dialog open, composer not active |
| `?` | `open-palette-help` | Open command palette | same; empty query is the cheat sheet |

Ignore `event.repeat` for every catalog command.

`n`, `j`/`k`/arrows, Space, Ctrl+A, Alt+1/2/3, Ctrl+N, Ctrl+/−/0 stay where
they are today. Do not duplicate them in the catalog.

### Palette-only (`keys: []`, `palette: true`)

| `id` | Title | `when()` |
|---|---|---|
| `compose` | Compose new message | composer not already active |
| `go-unified` | Go to Unified Inbox | always |

Dynamic rows (not static catalog entries) are built at filter time from
**folder rows**, not raw `Mailbox.name`. Add `sidebarFolderRows()` beside
`sidebarFolders` in `folderNav.ts`:
`flattenFolderTree(sidebarTree(folders), allExpandablePaths(tree))` filtered
to rows with `folder != null`. Title uses `row.label` (Inbox, Sent, …) and
`accounts.find(a => a.dbId === row.folder.accountId)?.name`.

- `Go to {account.name} / {row.label}` — always, including the current
  folder (calls `selectFolder`, which reloads).
- `Move to {account.name} / {row.label}` — only with a nonempty selection
  whose envelopes share one account; only folders on that account;
  excluding folders the selection already lives in (same skip as
  `moveSelectedToFolder`). Mixed-account selections omit every Move to
  row.

Hidden Gmail Other-role folders stay hidden (`sidebarTree` already drops
them).

### Focus exemptions

**Letter keys, `?`, Backspace, Delete, `#`, `!`:** do not run when

- focus is inside `input, textarea, select, [contenteditable], [role=dialog],
  [role=menu], [popover], [aria-haspopup=listbox]`, or
- `preferencesOpen`, `outboxAccountId`, add-account / folder dialogs (covered
  by `role=dialog`), or
- `activeComposerId !== null`.

**Ctrl+K / Cmd+K:** skip the same dialogs and an active composer (TipTap
keeps Ctrl+K for links). **Do** run when focus is in the thread-list search
`<input>`.

The mail iframe is not a protected target. Iframe keydown calls
`dispatchCommand`; a true return skips `handleMessageBodyKeydown`.

### Mark unread vs delayed Seen

`mark-unread` calls `setSelectedFlag("Seen", false)` and clears
`markReadTimer`. Without that, a pending delayed mark-as-read would flip the
still-open row back to Seen.

---

## §3 Palette UI

Compact sheet (option A):

- Overlay: `position: fixed; inset: 0; z-index: 80; background: var(--overlay)`;
  click on the dim area closes.
- Card: 520px wide, `max-height: min(480px, 70vh)`, upper-center
  (`align-items: start; padding-top: ~12vh`), paper gradient / border /
  `shadow-float` / `radius-lg` matching Preferences.
- `role="dialog"` `aria-modal="true"` `aria-label="Command palette"`
  `use:trapFocus`.
- Search field focused on open. Combobox pattern: input
  `role="combobox"` `aria-expanded="true"` `aria-controls` +
  `aria-activedescendant`; list `role="listbox"`; rows `role="option"`.
- Escape or Ctrl+K/Cmd+K while open toggles closed and restores the
  previously focused element (`trapFocus` already restores).
- App window Escape handles `commandPaletteOpen` **before** prefs, outbox,
  and composer minimize.

**Empty query (cheat sheet).** Static catalog entries with `palette: true`,
grouped Triage → Compose → Folders. Title left, chord right (`<kbd>`).
Disabled rows (`when() === false`) stay visible, muted, and are skipped by
arrow-key highlight. Initial highlight is the first `when() === true` row
so Enter-on-open is defined. Footer: `Enter run · Esc close`. No folder
rows.

**Typing.** Case-insensitive subsequence match over title, group, account
name, and folder label. Consecutive matches score above scattered;
word-start matches score above mid-word. No new dependency. Sort by score
then title. Cap 40. Filtered results show only `when() === true` rows.
Folder Go to / Move to rows appear **only when the query is non-empty**.

**Keyboard inside the palette.** ArrowUp/Down (and `j`/`k` when the search
field is empty) move highlight. Enter runs the highlighted row and closes.
Tab stays trapped. Typing always goes to the search field.

**Run.** Enter or click calls `run()`, then sets `commandPaletteOpen = false`.
Go to → `selectFolder` / `selectUnifiedInbox`. Move to →
`moveSelectedToFolder` (auto-advance included). Compose → `openComposer()`.

No matches: a single non-selectable row, “No matching commands”.

---

## §4 Auto-advance, unread-only, undo

Auto-advance runs inside `scheduleMessageAction`, so `e`, palette Move to,
and the Archive / Trash / Junk / Delete toolbar buttons share it. Star,
read/unread, reply, forward, and Go to do not.

### Next-row pick (before `removeSelectedLocally`)

If `selectedEnvelope != null` and its id is **not** in the target set, keep
it and collapse `selectedMessageIds` to `[selectedEnvelope.id]`. (Bulk
select with no open row, or `clearMessageView`, leaves
`selectedEnvelope === null` — that path uses the walk below, not `.id`.)

Otherwise:

```
first = index of the topmost target in app.value.envelopes
walk forward from first skipping target ids
else walk backward from first - 1
else null
```

That is the message that occupies the archived slot, including a remaining
thread sibling when only the head was removed. `expandedThreads` stays
ThreadList-local.

Then `removeSelectedLocally` drops the targets, and if `next` is non-null
call `selectEnvelopeExclusive(next)` (loads the body, starts delayed
mark-as-read on the new row). If `next` is null, selection and message stay
cleared (existing empty artwork).

**Unread-only.** Removed ids leave the page. Keep-selected-on-Seen does not
retain an archived id. The next row is whatever the current
`app.value.envelopes` still contains.

**Page boundary.** After the patch, if `envelopes.length === 0` and
`hasMoreEnvelopes`, call `loadMoreEnvelopes()` as today. `loadMoreEnvelopes`
already no-ops its append when `envelopeRequest` has moved. `z`
(`undoLastMessageAction`) increments `envelopeRequest` so an in-flight
page-2 cannot splice onto the restored list. The 6-second execute path
does **not** bump `envelopeRequest`, so a load after a committed archive
still applies; if that load leaves a nonempty page **and**
`envelopeRequest` is unchanged **and** `selectedMessageIds` is still empty,
`selectEnvelopeExclusive` the first envelope (do not clobber a restored
Ctrl+A selection). `undoLastMessageAction` also `patch({ envelopesLoadingMore: false })`
because `loadMoreEnvelopes` skips that clear when the request id moved.

**Undo.** Snapshot is still taken *before* removal, so `z` restores
envelopes, folder counts, and the previous selection. It does not revert a
Seen flip on the message we advanced onto. Six-second toast unchanged. A
new destructive action still flushes the pending execute first.

---

## §5 Error handling

- Catalog `run()` no-ops when `when()` is false (dispatcher never calls
  those). Toolbar buttons keep today’s `lastError` paths (`Archive folder
  not found`, cross-account move).
- Palette closes after a successful dispatch. A store `lastError` stays on
  the existing error card; the palette does not grow its own error UI.
- Iframe `dispatchCommand` swallows nothing: if `run()` throws, it surfaces
  as today (unhandled rejection / `lastError`).
- Opening the palette while another dialog is open is a no-op.

---

## §6 Files and tests

### Files

| File | Change |
|---|---|
| `ui/src/lib/commands.ts` | **New.** Catalog, `dispatchCommand`, fuzzy `paletteItems(query)`. |
| `ui/src/lib/commands.test.ts` | **New.** Keys, exemptions, fuzzy, when(). |
| `ui/src/lib/CommandPalette.svelte` | **New.** Compact sheet overlay. |
| `ui/src/lib/CommandPalette.test.ts` | **New.** Dialog, filter, Enter, close. |
| `ui/src/App.svelte` | Call `dispatchCommand`; mount palette; Escape order. |
| `ui/src/lib/MessageView.svelte` | Iframe `dispatchCommand`; toolbar `title`s from catalog chords. In a Trash folder with empty search, the trash icon runs `delete-permanent` (`deleteSelectedPermanently`) so the label and action match; otherwise it stays `trash`. |
| `ui/src/lib/stores.svelte.ts` | `commandPaletteOpen`; auto-advance in `scheduleMessageAction`; clear `markReadTimer` on explicit unread. |
| `ui/src/lib/stores.markread.test.ts` | Unread cancels the pending timer. |
| `ui/src/lib/ThreadList.svelte` | Optional bulk-bar titles from catalog; no second key map. |
| `ui/src/lib/folderNav.ts` | Add `sidebarFolderRows()` (flattened labelled rows, all expanded). Export or reuse `allExpandablePaths`. |
| `ui/src/lib/folderNav.test.ts` | Rows use sidebar labels, not IMAP `Mailbox.name`; hidden Gmail Other folders omitted. |

No Rust/Tauri changes. Reuse `sidebarFolders`, `trapFocus`, overlay tokens.

### Tests (Vitest)

`commands.test.ts`

- Matches `e`, `s`, `#`, `!`, Backspace, Delete, `r`, `a`, `f`, Shift+U,
  Shift+I, `z`, Ctrl+K, `?`.
- Ctrl+A does not also fire `reply-all`.
- Letter keys skipped in input / textarea / contenteditable / dialog /
  composer-active.
- Ctrl+K runs from a search `<input>`; skipped in contenteditable and when
  `activeComposerId` is set.
- `event.repeat` does not run.
- Reply `when()` false without a loaded matching message.
- Star: mixed selection stars; all-flagged unstars.
- Trash vs delete-permanent by folder role and search query.
- Empty query: catalog groups, no folder rows, `open-palette` omitted.
- Non-empty query: Go to / Move to from `sidebarFolderRows` / `FolderRow.label`;
  Move to omitted without selection and for other accounts; hidden Gmail
  folders omitted; cap 40.

`stores` / mark-read

- Archive selects the slot-next remaining envelope.
- Last row selects previous; last remaining row empties the pane.
- Unread-only does not keep the archived id on the page.
- Empty page with `hasMoreEnvelopes` loads more and selects the first.
- Undo restores envelopes and the previous selection, and bumps
  `envelopeRequest` so an in-flight `loadMoreEnvelopes` does not append.
- `selectedMessageIds` nonempty with `selectedEnvelope === null` (Ctrl+A /
  `clearMessageView`) walks / empties / loads more and does not throw.
- `setSelectedFlag("Seen", false)` clears `markReadTimer` so a delayed
  mark-as-read does not revive Seen.

`CommandPalette.test.ts`

- Dialog + `trapFocus`; Escape and overlay click close.
- Empty query shows grouped cheat sheet with chords.
- Typing filters; Enter runs the highlighted command and closes.

Covering check: `npm --prefix ui test -- --run` and `npm --prefix ui run check`.

---

## §7 Implementation notes for the planner

- Prefer a root-cause change in `scheduleMessageAction` over reconstructing
  next-row selection in Svelte components.
- Keep the UI provider-neutral; folder rows use sidebar names, not provider
  APIs.
- One regression per behavior above; do not add Playwright for this feature.
- Preserve unrelated dirty-worktree files. Do not commit unless asked.
