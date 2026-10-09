# Origami — Compose Tabs and Draft Persistence Design

Date: 2026-10-08
Status: approved design, pending implementation plan
Scope: replace the single compose modal with a read-pane dock of multiple
compose sessions (Outlook-style bottom tabs), fix draft saving/recovery
limbo, relocate the redesigned composer into the read column.

## Goals

1. **Multiple compose sessions** — new / reply / forward each open an
   independent session with its own draft, shown as a tab strip pinned to
   the bottom of the read pane.
2. **Draft persistence without limbo** — saved sessions restore as
   minimized tabs on launch; no "Recovered draft" banner, no blank ghost
   drafts, no silent clobbering of one global draft key.
3. **Composer redesign** — docked pane with recipient chips and
   validation, collapsible Cc/Bcc, grouped sticky toolbar (link, undo/redo),
   drag-and-drop attachments with inline image thumbnails, and honest
   save/sync status in the footer.
4. **Per-draft IMAP sync** — each session's draft syncs to the server
   Drafts folder under its own remote UID; send/discard removes its copy.

## Non-goals

- Reopening server-side draft messages from the Drafts folder list into a
  compose session (future work; those rows stay read-only messages).
- Projecting local drafts as rows in the thread list.
- Conflict resolution between local draft content and a server copy
  edited elsewhere.
- Scheduled send, undo send, encryption, or new protocol features.
- Reworking non-compose read-pane behavior or layouts.

## Baseline (as-built facts)

- `ui/src/lib/Composer.svelte` is a centered modal driven by global state
  (`stores.svelte.ts:93-117`), autosaving after 400 ms even when untouched
  (`Composer.svelte:23-42`).
- `openComposer()` auto-loads the one backend draft and flags
  `composerRecovered` (`stores.svelte.ts:1285-1331`); `closeComposer()`
  unconditionally re-saves and syncs it (`:1389-1410`). Replies overwrite
  the same key. That is the limbo state.
- Backend: `save/load/sync/delete_composer_draft` all use the fixed key
  `"composer"` (`crates/origami-app/src/commands.rs:1842-1974`). The
  `drafts` SQLite table is already keyed by id with per-id remote tracking
  columns (`crates/origami-core/src/store.rs:1990-2050`), so generalization
  does not need a schema change.
- `sync_composer_draft` already skips empty payloads and replaces the
  previous remote copy by UID.
- `RecipientInput.svelte` already renders chips, dedupes, and supports
  backspace removal; invalid tokens are silently dropped.
- TipTap 3 `StarterKit` bundles Link, Underline, and undo/redo; no new
  dependency needed (`ui/package.json`).
- No in-app confirm dialog component exists.

## §1 Interaction model and state

The read column holds one surface at a time: message, active compose, or
message with the tab strip when all sessions are minimized.

```
State additions:
  composerSessions: ComposerSession[]
  activeComposerId: string | null      // no active editor when null
  sendingComposerId: string | null     // replaces global `sending`
  discardConfirmId: string | null      // tab awaiting discard confirmation

ComposerSession:
  id: string                           // crypto.randomUUID()
  accountId: string | null
  draft: { to, cc, bcc, subject, html, composeMode }
  attachments: { name, mime, size, dataBase64 }[]
  threading: { inReplyTo, references }
  saveState: "idle" | "saving" | "saved" | "error"
  syncState: "local" | "syncing" | "synced" | "error"
  lastSyncedHash: string | null        // in-memory only
```

Removed state: `composerOpen`, `composerAccountId`, `composerDraft`,
`composerAttachments`, `composerThreading`, `composerRecovered`,
`composerDiscarded`, global `sending`, and the old `closeComposer`.

Behavior:

- `openComposer(seed?, threading?)` keeps its current call signature but
  always creates a **new** session and activates it. It never loads a
  stored draft and never sets a recovery flag.
- `activateComposer(id)` sets `activeComposerId`; clicking the active tab
  toggles minimize; `minimizeComposer()` sets `activeComposerId = null` and
  keeps the session.
- Message selection actions (`selectEnvelope`, `selectEnvelopeExclusive`,
  folder switches, search open) set `activeComposerId = null` so the
  message takes the read pane; the strip remains.
- `discardComposer(id)` deletes the local row and remote copy, removes the
  session, and activates the nearest remaining tab (right neighbor, else
  left, else none). Non-empty sessions ask for confirmation first
  (`discardConfirmId`); blank sessions close silently.
- `sendComposer(id)` sends that session, then deletes its local row and
  remote copy, then removes the tab. Failure keeps the tab and shows the
  existing error surface. `sendingComposerId` gates only that session's
  Send button, so in-flight sends do not block editing other tabs.
- Blank predicate (never persisted, never synced): recipient fields empty
  after trim, subject empty, body empty after stripping tags/whitespace
  (the server-side `<p></p>` rule), no attachments. A blank in-memory tab
  survives minimize but disappears on restart.
- Restore: `bootstrap()` calls `restoreComposerSessions()` after accounts
  load; every stored draft becomes a session with all tabs minimized.
- Keyboard: `N` opens a new session; `Escape` minimizes the active session
  (or dismisses the discard confirmation); `Ctrl+Enter` sends from the
  active pane.

## §2 Persistence and sync

Commands generalized from the fixed `"composer"` key to an explicit id:

- `save_composer_draft(id, draft)` — local save.
- `list_composer_drafts() -> [{ id, draft }]` — replaces
  `load_composer_draft`; sorted `updated_at DESC`.
- `delete_composer_draft(id)` — deletes local row and remote copy.
- `sync_composer_draft(id)` — appends/replaces the remote Drafts copy for
  that id, reusing the existing UID swap and empty-skip logic.

Store: add `list_drafts()` returning `(id, draft_json)` rows ordered by
`updated_at DESC`; existing per-id `save_draft` / `load_draft` /
`delete_draft` / `draft_remote` / `set_draft_remote` stay as-is.

Rules:

- **Legacy migration**: the old `"composer"` row is adopted as a session on
  first list. If it is blank under the predicate above, it is deleted
  (local + best-effort remote) so the current ghost-draft bug cannot
  reappear. No other schema or data migration.
- **Local autosave**: 400 ms debounce per session, skipped for blank
  sessions; `saveState` transitions `idle → saving → saved|error`; the
  legacy `origami-composer-draft` localStorage fallback is removed.
- **Remote sync**: after a successful local save, schedule a remote sync
  4 s later, coalesced per session and skipped when
  `composerPayloadHash(session) === lastSyncedHash`. Also force a sync on
  minimize. No sync on restore (the server copy already matches unless the
  draft changed). Hash is built from an explicit field-order payload, not
  raw proxy serialization.
- **Send/discard**: delete local row and remote copy; remote-delete failure
  does not block tab removal (warning toast).
- **Quit**: keep the existing `beforeunload` best-effort local flush.
  Remote sync is not awaited on quit; the local DB stays source of truth
  and the next edit or minimize re-syncs.
- Server Drafts rows remain read-only messages in the thread list; the
  remote copy is a backup, not an editable surface (non-goal above).

## §3 Compose pane redesign

Replace `Composer.svelte` with `ComposePane.svelte`, rendered in the read
column instead of the modal overlay. Top to bottom:

- **Header**: From account `Select` (compact), Cc/Bcc toggle, Minimize
  button. No modal chrome, no header title (the tab carries identity).
- **Recipients**: `RecipientInput` extended so every typed token commits as
  a chip; chips failing address validation get `aria-invalid` styling and
  a warning, and `sendComposer` blocks with an inline error naming them.
  Cc/Bcc chips are hidden until the toggle is used or the field is
  non-empty.
- **Subject**: text input styled larger than body fields.
- **Toolbar** (sticky below fields): Rich/Plain toggle; rich-only
  B/I/U, H2, bullet/ordered list, quote, code, Link, Undo, Redo; always
  available: inline image insert and Attach. Link opens a small in-app
  popover (URL + Apply/Remove), not a native prompt. Plain mode keeps the
  attach controls that today disappear.
- **Editor**: TipTap rich editor or plain textarea, keyed by session id so
  switching tabs remounts cleanly from store state.
- **Attachments**: files dropped anywhere on the pane become attachments
  (same 25 MB total rule); pasted images insert inline via the existing
  `fileToInlineImage` path; attachment cards show an image thumbnail from
  `dataBase64`, name, MIME, size, and remove; a running total warns as the
  limit approaches.
- **Footer**: left — status derived from `saveState`/`syncState`
  (`Saving…`, `Saved locally HH:MM`, `Synced to server`, `Sync failed ·
  Retry`); right — Discard (danger ghost) and Send (primary).
- **Discard confirmation**: inline in-pane confirm ("Discard draft?" Keep
  editing / Discard) for non-empty sessions, only when
  `discardConfirmId` matches; Escape cancels; focus moves to Keep editing
  and returns to the pane afterwards.
- Signature: inserted only when a session is created, as today; changing
  From never rewrites the body. The existing `-- ` guard is kept.
- Focus: newly created sessions focus To; activating a restored/other
  session focuses the pane region without stealing focus into fields.

Both panes and the strip follow the existing token system (paper/crease
surfaces, `--radius-*`, `--shadow-*`); the docked pane reads as a flat
raised surface rather than a floating sheet.

## §4 Tab strip

New `ComposeTabs.svelte`, pinned to the bottom of the read column and
rendered only when sessions exist.

- Item: tab button (`role="tab"`, `aria-selected`, `aria-controls`) plus an
  adjacent close button (`aria-label="Discard draft: <title>"`), drawn as
  one chip. Separate focus targets avoid nested interactive controls.
- Title: trimmed subject → first valid To recipient → `New message`;
  ellipsized with a full-title tooltip.
- Status dot: saving (animated), sync error (warning hue), hidden
  otherwise; always paired with an accessible label, never color alone.
- Interaction: click inactive → activate; click active → minimize; close →
  confirm when non-empty. Arrow Left/Right rove the tablist, Home/End jump,
  Enter/Space activate. The active tab scrolls into view.
- Overflow: horizontal scrolling, no wrapping, no session cap. Native
  scrollbar plus Shift+wheel; no `preventDefault` wheel hijack.
- Styling: active tab gets an accent top edge and raised background;
  inactive tabs are muted. Strip height stays constant when empty
  (not rendered) or populated.
- Responsive: at ≤760px the read column spans the workspace when a compose
  session is active (`composer-active` class on `.pane-row` hides the
  thread list); the existing "Back to messages" action minimizes the
  active composer first; the Compose FAB is hidden while the pane is
  active to avoid overlapping the footer.

## §5 App integration

- `App.svelte` gains a `.read-column` wrapper: `MessageView` (kept mounted,
  `hidden` while a compose session is active, preserving iframe and scroll
  state) + `ComposePane` (lazy-imported on first session, as the modal is
  today) + `ComposeTabs`.
- The lazy-import effect keys off `composerSessions.length > 0` instead of
  `composerOpen`.
- `Back to messages` calls `minimizeComposer()` when a session is active,
  then performs its current behavior.
- Replace every remaining `composerOpen` guard with
  `activeComposerId !== null`: App keyboard handling (Escape, `N`,
  Alt+1/2/3 gating) and `ThreadList.svelte`'s keydown suppression. No
  `composerOpen` field survives the refactor.

## Verification

Frontend (`cd ui && npm test`, `npm run check`):

- `stores.composer.test.ts` (replaces `stores.draft.test.ts`): restore maps
  stored drafts to minimized sessions; legacy `"composer"` row adopted;
  blank legacy row deleted; blank sessions never persisted; per-session
  autosave debounce; payload-hash sync skip; discard/send remove local and
  remote; discard confirmation flow; nearest-tab activation after removal.
- `ComposeTabs.test.ts`: titles, activation, active-tab minimize, close
  confirm, roving keyboard navigation, status-dot labels.
- `ComposePane.test.ts` (replaces `Composer.test.ts`): new-session focus,
  Cc/Bcc toggle, invalid-recipient blocking, drag-drop attach, discard
  confirm, footer status mapping.
- `composer-test-store.svelte.ts` rewritten around sessions.

Backend (`cargo test -p origami-core -p origami-app`):

- `list_drafts` ordering and per-id isolation regression test in
  `store.rs`.
- Existing compose/sync tests updated for the id-parameterized commands.

Manual smoke (documented in the implementation plan): open three sessions,
type into each, restart → three minimized tabs with content; discard one →
others untouched; send one → its local row and server copy gone; open and
close an untouched composer → nothing restored next launch.

## Delivery slices

1. **Backend generalization** — `list_drafts`, id-parameterized
   save/sync/delete, remove `load_composer_draft`, tests. UI unchanged.
2. **Session state + persistence** — session model, open/activate/minimize/
   discard/send, autosave, restore + legacy migration, tab strip. Pane
   still the old form content hosted in the read column.
3. **Pane redesign** — chips validation, Cc/Bcc, toolbar/link/undo,
   drag-drop attachments with thumbnails, footer status, discard confirm.
4. **Docs** — update `docs/wiki/architecture/ui-state-and-rendering.md`
   and append a `docs/wiki/log.md` entry per the wiki schema.

## Cross-cutting rules

- UI stays provider-neutral; draft rules live in `origami-core` /
  `origami-app`, not reconstructed in Svelte.
- Preserve unrelated dirty-worktree changes (attachment-open fix, date
  formatting, wiki edits currently pending); do not commit unless
  explicitly requested.
- One runnable regression check per fix.
- Read `docs/adr/` before touching sync semantics; per-draft sync only
  parameterizes the existing append/swap path, no algorithm change.

## Decisions

- **Source of truth**: the local `drafts` table plus in-memory sessions;
  the IMAP Drafts copy is a backup synced per draft.
- **Wire format**: recipient fields stay comma-separated strings, so
  `PersistedDraftFields` and `build_draft_message` are unchanged.
- **IDs**: client-generated UUIDs; the legacy `"composer"` id is adopted
  rather than rewritten.
- **No server-draft reopening** in this change; future work layers on the
  same per-id model.
- **Tab strip placement**: bottom of the read column, per approved design.

## Open questions (non-blocking)

- Whether the strip later gains a `+` button (today only the FAB opens new
  sessions).
- Whether blank in-memory tabs should be capped or auto-dropped rather than
  lingering until restart.
