# Compose Tabs and Draft Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single compose modal with a read-pane dock of multiple per-draft compose sessions (bottom tab strip) and fix draft save/restore limbo.

**Architecture:** The local `drafts` SQLite table (already keyed by id) becomes a per-draft store. Tauri commands gain an `id` parameter; the frontend holds `composerSessions[]` with one active editor rendered in the read column, plus a tab strip. Sending/discarding deletes the row and the remote Drafts copy.

**Tech Stack:** Rust (Tauri 2, rusqlite), Svelte 5 runes, TypeScript, TipTap 3, Vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-10-08-compose-tabs-and-drafts-design.md` — read it with this plan.

## Global Constraints

- No new dependencies; TipTap `StarterKit` v3 already bundles Link, Underline, Undo/Redo.
- Recipient wire format stays comma-separated strings; `PersistedDraftFields` and `build_draft_message` are untouched.
- UI stays provider-neutral; draft rules live in `origami-app`/`origami-core`, not reconstructed in Svelte.
- One runnable regression check per behavior change.
- Preserve unrelated dirty-worktree changes (`commands.rs` attachment LRU fix, `formatDate` usage, wiki edits); do not revert or reformat them.
- **Do not commit unless the user explicitly asks.** Each task's checkpoint step is a no-op unless told otherwise.
- Blank-draft predicate: all recipients empty after trim, subject empty, body empty after stripping tags, `&nbsp;`, and whitespace, no attachments.

## Review Focus

Inputs/conditions the spec implies but no happy-path test covers, most likely first:

1. **Restore with zero accounts**: drafts on disk must not be deleted or lost; skip restoring until an account exists.
2. **Rapid tab switches with pending autosaves**: each session's debounced save must use its own id; switching must not cancel or misfile another session's save.
3. **Corrupt legacy localStorage draft**: unparseable JSON must be ignored and not crash bootstrap.
4. **Remote sync failure then Retry**: `syncState` must reach `error`, Retry must re-attempt and reach `synced`.
5. **Whitespace-only body**: `<p>&nbsp;</p>` / `<p>  </p>` must count as blank (never persisted).

---

### Task 1: Store `list_drafts`

**Files:**
- Modify: `crates/origami-core/src/store.rs` (add method after `load_draft`, ~line 2010; test in the file's existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: existing `Store::open_in_memory()`, `save_draft(id, json)`.
- Produces: `Store::list_drafts() -> Result<Vec<(String, String)>>` — all rows, newest first: `(id, draft_json)`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn list_drafts_returns_newest_first_with_ids() {
    let store = Store::open_in_memory().unwrap();
    store.save_draft("a", "{\"v\":1}").unwrap();
    store.save_draft("b", "{\"v\":2}").unwrap();

    let rows = store.list_drafts().unwrap();

    assert_eq!(
        rows.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        vec!["b", "a"]
    );
    assert_eq!(rows[0].1, "{\"v\":2}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p origami-core list_drafts_returns_newest_first_with_ids`
Expected: FAIL — no method `list_drafts`.

- [ ] **Step 3: Implement `list_drafts` in `crates/origami-core/src/store.rs`**

```rust
pub fn list_drafts(&self) -> Result<Vec<(String, String)>> {
    let conn = self.conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, draft_json FROM drafts ORDER BY updated_at DESC, rowid DESC",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
```

(`updated_at` is `unixepoch()` — second resolution — so `rowid DESC` is the deterministic tiebreak the test asserts.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p origami-core list_drafts_returns_newest_first_with_ids`
Expected: PASS

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 2: Parameterize draft commands by id

**Files:**
- Modify: `crates/origami-app/src/commands.rs:1841-1974`
- Modify: `crates/origami-app/src/lib.rs:307-310`
- Modify: `ui/src/lib/api.ts:98-118, 197-201`
- Modify: `ui/src/lib/stores.svelte.ts` (legacy call sites: `discardComposerDraft`, `closeComposer`, `sendComposer` — pass `LEGACY_COMPOSER_DRAFT_ID`)

**Interfaces:**
- Consumes: `Store::list_drafts` (Task 1), existing `PersistedComposerDraft`, `build_draft_message`.
- Produces (Tauri): `save_composer_draft(id, draft)`, `list_composer_drafts() -> [{id, draft}]`, `sync_composer_draft(id)`, `delete_composer_draft(id)`; `load_composer_draft` is removed.
- Produces (TS): `StoredComposerDraft { id: string; draft: SavedComposerDraft }`; `api.saveComposerDraft(id, draft)`, `api.listComposerDrafts()`, `api.syncComposerDraft(id)`, `api.deleteComposerDraft(id)`.

- [ ] **Step 1: Update `commands.rs`**

Replace the four commands with:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedComposerDraftDto {
    pub id: String,
    pub draft: serde_json::Value,
}

#[tauri::command]
pub fn save_composer_draft(state: State<'_, AppState>, id: String, draft: serde_json::Value) -> CmdResult<()>

#[tauri::command]
pub fn list_composer_drafts(state: State<'_, AppState>) -> CmdResult<Vec<SavedComposerDraftDto>>

#[tauri::command]
pub async fn sync_composer_draft(state: State<'_, AppState>, id: String) -> CmdResult<()>

#[tauri::command]
pub async fn delete_composer_draft(state: State<'_, AppState>, id: String) -> CmdResult<()>
```

Bodies: every `"composer"` literal becomes `&id` / `id.as_str()`. `list_composer_drafts` maps `state.store.list_drafts()` rows through `serde_json::from_str` for `draft`. Delete the `load_composer_draft` function; keep `PersistedComposerDraft` (sync still uses it). Keep the empty-payload early-return in sync and the remote UID swap exactly as-is.

- [ ] **Step 2: Register in `lib.rs`**

Replace `commands::load_composer_draft` with `commands::list_composer_drafts` in the `generate_handler!` list.

- [ ] **Step 3: Update `api.ts`**

Add `StoredComposerDraft`; change the four wrappers to the signatures in the Interfaces block; delete `loadComposerDraft`.

- [ ] **Step 4: Keep legacy UI compiling**

In `stores.svelte.ts`, add `export const LEGACY_COMPOSER_DRAFT_ID = "composer";` and update the existing `discardComposerDraft` / `closeComposer` / `sendComposer` calls to pass it as the first argument. No behavior change yet.

- [ ] **Step 5: Verify**

Run: `cargo test -p origami-core -p origami-app` → PASS.
Run: `cd ui && npm test` → PASS.
Run: `cd ui && npm run check` → no errors.

- [ ] **Step 6: Checkpoint** — do not commit unless explicitly asked.

---

### Task 3: Sessions cutover — multi-draft state, docked pane, tab strip

The old modal is deleted in this task; the branch stays green because store, components, and App switch together.

**Files:**
- Modify: `ui/src/lib/stores.svelte.ts` (replace composer block, lines ~93-117, 160-169, 1261-1460; add restore to `bootstrap`)
- Create: `ui/src/lib/ComposePane.svelte` (from `Composer.svelte`, no overlay)
- Create: `ui/src/lib/ComposeTabs.svelte`
- Modify: `ui/src/App.svelte`, `ui/src/lib/MessageView.svelte`, `ui/src/lib/ThreadList.svelte:94`
- Rewrite: `ui/src/lib/composer-test-store.svelte.ts`
- Delete: `ui/src/lib/Composer.svelte`, `ui/src/lib/Composer.test.ts`, `ui/src/lib/stores.draft.test.ts`
- Test: `ui/src/lib/stores.composer.test.ts` (new), `ui/src/lib/ComposeTabs.test.ts` (new), `ui/src/lib/ComposePane.test.ts` (new), `ui/src/lib/ThreadList.test.ts` (mock state)
- Modify: `ui/src/lib/stores.svelte.ts` selection functions (`selectFolder` ~471, `selectEnvelope` ~656, `searchMessages` ~783, and unified-inbox load) to include `activeComposerId: null`

**Interfaces:**
- Consumes: Task 2 api (`StoredComposerDraft`, four wrappers).
- Produces (exports from `stores.svelte.ts`):

```ts
export interface ComposerAttachment { name: string; mime: string; size: number; dataBase64: string }

export interface ComposerSession {
  id: string;
  accountId: string | null;
  draft: { to: string; cc: string; bcc: string; subject: string; html: string; composeMode: "rich" | "plain" };
  attachments: ComposerAttachment[];
  threading: { inReplyTo: string | null; references: string[] };
  saveState: "idle" | "saving" | "saved" | "error";
  syncState: "local" | "syncing" | "synced" | "error";
  lastSavedAt: number | null;
  lastSyncedHash: string | null;
}

// State: composerSessions, activeComposerId, sendingComposerId, discardConfirmId
export function openComposer(draft?: Partial<ComposerSession["draft"]>, threading?: Partial<ComposerSession["threading"]>): void
export function activateComposer(id: string): void
export function minimizeComposer(): void
export function requestDiscardComposer(id: string): void
export function cancelDiscardComposer(): void
export function discardComposer(id: string): Promise<void>
export function sendComposer(id: string): Promise<void>
export function restoreComposerSessions(): Promise<void>
export function scheduleComposerSave(id: string): void
export function syncComposerNow(id: string): Promise<void>
```

Removed exports/fields: `composerOpen`, `composerAccountId`, `composerDraft`, `composerAttachments`, `composerThreading`, `composerRecovered`, `composerDiscarded`, `sending`, `closeComposer`, `discardComposerDraft`.

Behavior details the implementer must follow:

- `restoreComposerSessions()`: return early without API calls when `app.value.accounts.length === 0` (Review Focus 1). Otherwise load rows; for each, build a session (blank-fill missing fields, `composeMode: "rich"`, `saveState: "saved"`, `syncState: "local"`, `lastSyncedHash: null`). Blank rows → best-effort `api.deleteComposerDraft(row.id)` and skip. Legacy id `"composer"` is adopted as a session id. Then migrate `localStorage["origami-composer-draft"]` if present: parse; on parse failure remove it and continue (Review Focus 3); on success build a session with a fresh id, only if non-blank, then remove the key. All sessions start with `activeComposerId: null`.
- Autosave: `scheduleComposerSave(id)` debounces 400 ms per id into `flushComposerSave(id)`. Flush skips blank sessions, sets `saveState`, snapshots explicit plain fields (no proxies) for `api.saveComposerDraft(id, snapshot)`, sets `lastSavedAt` on success and schedules sync, `saveState: "error"` otherwise. Pending timers are per-id so a tab switch cannot cancel or misfile another session's save (Review Focus 2). Keep the old modal's `beforeunload` best-effort flush: `ComposePane` adds a window listener that flushes the active session immediately and removes it on destroy.
- Remote sync: after a successful save schedule a per-id 4 s debounce; `syncComposerNow(id)` (Retry) bypasses the delay. Both skip when `payloadHash(session) === session.lastSyncedHash && syncState === "synced"`; on failure `syncState = "error"` (Review Focus 4). `minimizeComposer()` flushes then syncs the minimized session.
- Blank predicate: strip tags via `html.replace(/<[^>]*>/g, "")`, replace `&nbsp;` with a space, trim; empty plus empty recipients/subject and zero attachments → blank (Review Focus 5).
- `sendComposer(id)`: guard `sendingComposerId`; validate each parsed recipient against `/^[^\s@]+@[^\s@]+\.[^\s@]+$/` and fail with `lastError = "invalid recipient: <value>"`; send with session fields; on success best-effort `api.deleteComposerDraft(id)` then remove the session; on failure keep it. `sendingComposerId` gates only that session.
- `discardComposer(id)`: nearest-tab activation = right neighbor after removal, else left, else null. Blank sessions discard silently; non-blank only via `requestDiscardComposer` → `discardConfirmId`. A failed remote delete still removes the tab and sets `lastNotice = "Draft removed locally; server copy may remain"`.
- `openComposer` keeps reply/forward semantics: resolve account from selected folder → session accountId; append signature with the existing `-- ` guard; never load stored drafts and never set a recovery flag.

Components:

- `ComposePane.svelte`: renders only when `activeComposerId` resolves; `{#key session.id}` around the TipTap mount; fields bind to the session; footer shows status text from `saveState`/`syncState` and `lastSavedAt`; Send calls `sendComposer(session.id)`; Discard calls `requestDiscardComposer(session.id)`; confirm bar when `discardConfirmId === session.id`; minimize button calls `minimizeComposer()`. Autosave `$effect` reads the session payload and calls `scheduleComposerSave(session.id)`. A pane container with `tabindex="-1"` receives focus when an existing session is activated (new sessions focus To instead).
- `ComposeTabs.svelte`: tab strip for `composerSessions`; click activates, clicking the active tab minimizes, close calls `requestDiscardComposer(id)`; rendered only when sessions exist.
- `App.svelte`: `.read-column` wrapper (`MessageView` in a `hidden` wrapper when a session is active, `ComposePane` lazy-imported when `composerSessions.length > 0`, `ComposeTabs` at the bottom). Escape → `minimizeComposer()`; Alt+1/2/3 guard becomes `activeComposerId !== null`; `N` keeps only the existing editable-target guard (pressing it outside a field opens a new session, matching the spec). Add `composer-active` class for the ≤760px rules.
- `MessageView.svelte`: `backToMessages` minimizes an active composer first.
- `ThreadList.svelte:94`: guard becomes `app.value.activeComposerId !== null`.

- [ ] **Step 1: Rewrite `composer-test-store.svelte.ts` for sessions**

Expose a mutable `app` with `composerSessions`, `activeComposerId`, `sendingComposerId`, `discardConfirmId`, `accounts`, `folders`, `selectedFolderId`, `correspondents`, `lastError`, `lastNotice`; mock exports `activateComposer`, `minimizeComposer`, `requestDiscardComposer`, `cancelDiscardComposer`, `discardComposer`, `sendComposer`, `scheduleComposerSave`, `syncComposerNow`, `openComposer`, `openReplyComposer`; `resetComposerTestStore()`.

- [ ] **Step 2: Write failing store tests in `stores.composer.test.ts`**

Mock `./api` (list/save/sync/delete composer draft, listFolders, listAccounts, sendMessage). Shared helpers: `seedAccount()`, and

```ts
const saved = (fields: Partial<SavedComposerDraft["draft"]>) => ({
  accountId: "account-1",
  draft: { to: "", cc: "", bcc: "", subject: "", html: "<p></p>", composeMode: "rich" as const, ...fields },
  attachments: [],
  threading: { inReplyTo: null, references: [] },
});
```

Represent each assertion compactly:

```ts
it("restore_skips_without_accounts", async () => {
  app.value.accounts = [];
  await restoreComposerSessions();
  expect(api.listComposerDrafts).not.toHaveBeenCalled();
});

it("restore_maps_rows_to_minimized_sessions_and_drops_blank", async () => {
  seedAccount();
  api.listComposerDrafts.mockResolvedValue([
    { id: "b", draft: saved({ subject: "real" }) },
    { id: "a", draft: saved({}) },
  ]);
  await restoreComposerSessions();
  expect(app.value.composerSessions.map((s) => s.id)).toEqual(["b"]);
  expect(app.value.activeComposerId).toBeNull();
  expect(api.deleteComposerDraft).toHaveBeenCalledWith("a");
});

it("pending_saves_are_per_session", async () => {
  vi.useFakeTimers();
  openComposer({ subject: "one" });
  const first = app.value.composerSessions[0].id;
  openComposer({ subject: "two" });
  scheduleComposerSave(first);
  scheduleComposerSave(app.value.composerSessions[1].id);
  await vi.runAllTimersAsync();
  expect(api.saveComposerDraft).toHaveBeenCalledWith(first, expect.objectContaining({ draft: expect.objectContaining({ subject: "one" }) }));
  expect(api.saveComposerDraft).toHaveBeenCalledWith(app.value.composerSessions[1].id, expect.anything());
});

it("blank_body_counts_as_blank", async () => {
  openComposer({ html: "<p>&nbsp;</p>" });
  scheduleComposerSave(app.value.composerSessions[0].id);
  await vi.runAllTimersAsync();
  expect(api.saveComposerDraft).not.toHaveBeenCalled();
});

it("corrupt_legacy_localstorage_is_dropped", async () => {
  seedAccount();
  api.listComposerDrafts.mockResolvedValue([]);
  localStorage.setItem("origami-composer-draft", "{not json");
  await restoreComposerSessions();
  expect(app.value.composerSessions).toEqual([]);
  expect(localStorage.getItem("origami-composer-draft")).toBeNull();
});

it("legacy_composer_row_is_adopted", async () => {
  seedAccount();
  api.listComposerDrafts.mockResolvedValue([{ id: "composer", draft: saved({ subject: "old" }) }]);
  await restoreComposerSessions();
  expect(app.value.composerSessions[0].id).toBe("composer");
});

it("discard_confirms_then_removes_and_activates_neighbor", async () => {
  seedAccount();
  openComposer({ subject: "a" }); openComposer({ subject: "b" });
  const [first, second] = app.value.composerSessions.map((s) => s.id);
  requestDiscardComposer(first);
  expect(app.value.discardConfirmId).toBe(first);
  await discardComposer(first);
  expect(api.deleteComposerDraft).toHaveBeenCalledWith(first);
  expect(app.value.composerSessions.map((s) => s.id)).toEqual([second]);
  expect(app.value.activeComposerId).toBe(second);
});

it("send_removes_session_and_draft", async () => {
  seedAccount();
  vi.mocked(api.sendMessage).mockResolvedValue({ queued: false });
  openComposer({ to: "a@example.org" });
  const id = app.value.composerSessions[0].id;
  await sendComposer(id);
  expect(api.deleteComposerDraft).toHaveBeenCalledWith(id);
  expect(app.value.composerSessions).toEqual([]);
});

it("send_blocks_invalid_recipient", async () => {
  seedAccount();
  openComposer({ to: "nope" });
  await sendComposer(app.value.composerSessions[0].id);
  expect(api.sendMessage).not.toHaveBeenCalled();
  expect(app.value.lastError).toContain("invalid recipient");
});

it("sync_error_then_retry_recovers", async () => {
  seedAccount();
  api.listComposerDrafts.mockResolvedValue([{ id: "x", draft: saved({ subject: "s" }) }]);
  await restoreComposerSessions();
  vi.mocked(api.syncComposerDraft)
    .mockRejectedValueOnce(new Error("offline"))
    .mockResolvedValueOnce(undefined);
  await syncComposerNow("x");
  expect(app.value.composerSessions[0].syncState).toBe("error");
  await syncComposerNow("x");
  expect(app.value.composerSessions[0].syncState).toBe("synced");
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd ui && npm test -- src/lib/stores.composer.test.ts`
Expected: FAIL — missing exports/state.

- [ ] **Step 4: Implement the store session model**

Replace the composer block per the Interfaces and behavior details above; wire `restoreComposerSessions()` into `bootstrap()` after accounts load (before/after folder selection, either is fine as long as accounts are populated); add `activeComposerId: null` to the selection patches. Reuse `htmlEscape`, `blankDraft`, signal logic from the old block; delete `legacySavedDraft`.

- [ ] **Step 5: Run store tests**

Run: `cd ui && npm test -- src/lib/stores.composer.test.ts`
Expected: PASS.

- [ ] **Step 6: Build `ComposePane.svelte` and `ComposeTabs.svelte`, wire App/MessageView/ThreadList**

Move `Composer.svelte` markup into `ComposePane.svelte` minus overlay/trapFocus; bind every field through the active session; keep existing attachment add/remove behavior for this task (redesign lands in Task 4). App integration and guard swaps per the component list. Delete the old modal and its tests.

- [ ] **Step 7: Rewrite component tests**

`ComposeTabs.test.ts`: renders one tab per session; clicking an inactive tab calls `activateComposer(id)`; clicking the active tab calls `minimizeComposer()`; closing a non-blank tab calls `requestDiscardComposer(id)`. `ComposePane.test.ts`: renders the active session's subject; To field focused on mount (keep the existing TipTap mock); Minimize calls `minimizeComposer`; footer shows `Saved locally` when `saveState === "saved"`. Update `ThreadList.test.ts` mock state to `activeComposerId: null`.

- [ ] **Step 8: Verify the cutover**

Run: `cd ui && npm test` → PASS.
Run: `cd ui && npm run check` → no errors.
Run: `cargo test -p origami-core -p origami-app` → PASS.

- [ ] **Step 9: Checkpoint** — do not commit unless explicitly asked.

---

### Task 4: Pane redesign extras

**Files:**
- Modify: `ui/src/lib/ComposePane.svelte`, `ui/src/lib/RecipientInput.svelte`, `ui/src/lib/ComposeTabs.svelte`, `ui/src/App.svelte` (responsive CSS/FAB)
- Modify: `ui/src/lib/RecipientInput.test.ts`, `ui/src/lib/ComposePane.test.ts`, `ui/src/lib/ComposeTabs.test.ts`
- Modify: `ui/src/lib/stores.svelte.ts` only if a new export is needed (`syncComposerNow` already exists)

**Interfaces:**
- Consumes: Task 3 sessions API and components.
- Produces: `RecipientInput` now commits every non-empty trimmed token as a chip; chips failing `/^[^\s@]+@[^\s@]+\.[^\s@]+$/` get class `invalid`, `aria-invalid="true"`, and a `title="Not a valid email address"`.

Behavior details:

- Cc/Bcc hidden behind a toggle until used or non-empty; Subject styled larger than body fields.
- `Ctrl+Enter` inside the pane sends the active session (`sendComposer(session.id)`).
- Toolbar: Rich/Plain toggle always; rich-only B/I/U/H2/lists/quote/code/Link/Undo/Redo (`toggleUnderline`, `setLink`, `unsetLink`, `undo`, `redo` from StarterKit v3); inline image and Attach always visible. Link uses an inline URL input row (Apply/Remove), no native prompt.
- Tab strip polish: `role="tablist"`/`role="tab"` with `aria-selected` and `aria-controls="compose-pane-<id>"`; close button is a sibling focus target; Arrow Left/Right rove focus, Home/End jump, Enter/Space activate; active tab `scrollIntoView({ block: "nearest", inline: "nearest" })`; status dot (animated saving / warning sync error) with an accessible label; active styling uses an accent top edge; horizontal overflow only, no wheel `preventDefault`.
- Responsive: at ≤760px `App.svelte`'s `composer-active` class forces the read column visible and hides the thread list, and the Compose FAB is hidden while a session is active.
- Drag-and-drop files on the pane root add attachments through the same size-guarded path as the picker; image attachments render a `data:${mime};base64,...` thumbnail in the card; a running total is shown and the 25 MB check remains `total + size > limit`.
- Paste/drop image into the editor inserts an inline image via `fileToInlineImage`.
- Footer sync status: `Synced to server` when `syncState === "synced"`, `Sync failed · Retry` button calling `syncComposerNow(id)` when `"error"`, `Saving…` while `saveState === "saving"`, else `Saved locally` with `lastSavedAt` time.
- Discard confirmation bar (created in Task 3 if not yet present) has Keep editing focused and returns focus to the pane.

- [ ] **Step 1: Update failing tests first**

`RecipientInput.test.ts`: replace `dedupes_and_ignores_non_addresses` with `commits_invalid_token_as_flagged_chip`:

```ts
await fireEvent.input(input, { target: { value: "not-an-address" } });
await fireEvent.keyDown(input, { key: "Enter" });
expect(onChange).toHaveBeenCalledWith("not-an-address");
// re-rendered chip has aria-invalid="true"
```

`ComposePane.test.ts` add: `shows_sync_retry_on_error` (set `syncState: "error"`, click Retry, expect `syncComposerNow(id)` mocked call); `drop_adds_attachment` (dispatch `drop` with one `File` on the pane root, expect it in the attachment list); `attach_at_25mb_boundary` (a file making the total exactly 25 MB is accepted; 1 byte more is rejected with `lastError`); `cc_bcc_hidden_until_toggled`; `ctrl_enter_sends` (keydown `Ctrl+Enter` calls `sendComposer(id)`); `beforeunload_flushes_pending_save` (dispatch `beforeunload`, expect a save for the active session).

`ComposeTabs.test.ts` add: `arrow_keys_rove_focus` (ArrowRight moves focus to the next tab); `active_tab_is_aria_selected`; `status_dot_reports_sync_error` (a session with `syncState: "error"` exposes a `Sync failed` label).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ui && npm test -- src/lib/RecipientInput.test.ts src/lib/ComposePane.test.ts src/lib/ComposeTabs.test.ts`
Expected: FAIL on the new assertions.

- [ ] **Step 3: Implement the redesign**

Follow the behavior details; keep the existing TipTap test mock pattern in `ComposePane.test.ts`. Do not change the `onChange` wire format — `RecipientInput` still emits comma-joined strings.

- [ ] **Step 4: Verify**

Run: `cd ui && npm test` → PASS.
Run: `cd ui && npm run check` → no errors.

- [ ] **Step 5: Checkpoint** — do not commit unless explicitly asked.

---

### Task 5: Wiki docs

**Files:**
- Modify: `docs/wiki/architecture/ui-state-and-rendering.md` (compose state section)
- Modify: `docs/wiki/log.md` (append ingest entry per `docs/wiki/SCHEMA.md`)

**Interfaces:**
- Consumes: the implemented behavior.
- Produces: wiki text matching the new state model; no code changes.

- [ ] **Step 1: Update the architecture page** — replace the single-composer description with sessions, per-draft persistence, restore-as-tabs, and discard/send cleanup; cite `stores.svelte.ts`, `ComposePane.svelte`, `ComposeTabs.svelte`, and the id-parameterized commands.
- [ ] **Step 2: Append the log entry** — date, what was ingested/changed, links to the page and this spec.
- [ ] **Step 3: Checkpoint** — do not commit unless explicitly asked.

---

## Final verification (run after all tasks)

```bash
cd ui && npm test
cd ui && npm run check
cargo test -p origami-core -p origami-app
```

Manual smoke: open three sessions, type distinct content, restart → three minimized tabs restore; discard one → others untouched; send one → its local row and server copy gone; open and close an untouched composer → nothing restores next launch; no "Recovered draft" banner ever appears.
