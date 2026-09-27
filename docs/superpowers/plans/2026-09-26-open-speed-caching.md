# Open-Speed Caching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Opening an email paints content as fast as possible: LRU-cached opens near-instant, list walking stays cache-warm via predictive + viewport prefetch, headers paint before bodies.

**Architecture:** A display LRU in `origami-app` sits in front of the SQLite parsed-message cache. A priority prefetch queue in `origami-core` fetches display MIME for predicted (hover/keyboard) and viewport rows without touching flags. The UI enqueues; Rust fetches, caches, and back off on throttle.

**Tech Stack:** Rust (rusqlite, tokio), Tauri 2 commands, Svelte 5 runes + vitest.

**Spec:** `docs/superpowers/specs/2026-09-26-hardening-caching-palette-design.md` (Track A)

## Global Constraints

- UI stays provider-neutral; prefetch/queue logic lives in `origami-core`/`origami-app`, never in Svelte ([[backend-seam]]).
- Prefetch must never write `Seen` or any flags.
- Open path priority: user open preempts prefetch; prefetch pauses while an open fetch is in flight.
- All prefetch stops when offline or the account is in error state.
- Existing constants stay: `PREFETCH_WINDOW` 7 days, `PREFETCH_MAX_MESSAGES` 500, `PREFETCH_MAX_MESSAGE_SIZE` 8 MiB.
- LRU caps: 32 MiB bytes / 300 entries. Display-cache caps: 2,000 rows or 30 days received-age, whichever binds first.
- One runnable regression check per fix.
- Commits only when the executor is explicitly authorized to commit; otherwise leave the worktree dirty.

## Review Focus

- Fast mouse sweep across the list enqueues hundreds of fetches → debounce + queue dedupe must collapse them to at most one in-flight fetch per message (`queue_dedupes_same_message`).
- User clicks a row while viewport prefetch is mid-flight → the open fetch must not wait behind queued prefetch (`open_overtakes_prefetch`).
- Folder switch mid-prefetch → no store writes for the abandoned folder after cancel (`cancel_stops_writes`).
- Provider throttle (`*BYE`, quota error) during prefetch → backoff and eventual prefetch disable; open path unaffected (`throttle_backoff_disables_prefetch`).
- Open an already-displayed message twice → second open must not call the store loader (`cache_hit_skips_loader`), including after eviction pressure did not evict the just-used entry.

---

### Task 1: `DisplayLru` with byte accounting

**Files:**
- Create: `crates/origami-app/src/display_lru.rs`
- Test: in-module `#[cfg(test)]` tests

**Interfaces:**
- Consumes: `origami_core::message::ParsedMessage` (existing type).
- Produces: `pub struct DisplayLru`; `pub fn new(max_bytes: usize, max_entries: usize) -> Self`; `pub fn get(&mut self, key: &str) -> Option<ParsedMessage>` (clones, refreshes recency); `pub fn insert(&mut self, key: String, value: ParsedMessage)` (evicts LRU until under both caps); `pub fn parsed_with_cache(cache: &mut DisplayLru, key: &str, load: impl FnOnce() -> anyhow::Result<Option<ParsedMessage>>) -> anyhow::Result<Option<ParsedMessage>>` (loads and inserts on `Some`). `ParsedMessage` approximate size via `value.html.len() + value.text.len()` (add fields if the struct carries plain-text/HTML buffers; whatever fields exist, the estimate is the sum of owned string buffers).

- [ ] **Step 1: Write failing tests** — `cache_hit_skips_loader` (`parsed_with_cache` twice with a counting `load` closure: second call must not invoke `load`), `evicts_by_entry_cap` (insert 4 into `new(_, 3)`: first key gone, last three present), `evicts_by_byte_cap` (insert values past `max_bytes`: total owned bytes of survivors ≤ cap), `get_refreshes_recency` (get(old) then insert overflow: old survives, untouched entry evicted).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-app display_lru`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `DisplayLru` in `crates/origami-app/src/display_lru.rs`**

Use `std::collections::HashMap` + `VecDeque` of keys (or `lru` crate if already a dependency — do not add a new dependency for this). Recency order: front = most recent.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p origami-app display_lru`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-app/src/display_lru.rs
git commit -m "feat: add display LRU with byte accounting"
```

### Task 2: Wire LRU into the open path

**Files:**
- Modify: `crates/origami-app/src/lib.rs` (add `pub display_lru: Mutex<DisplayLru>` to `AppState`, init `DisplayLru::new(32 * 1024 * 1024, 300)`)
- Modify: `crates/origami-app/src/commands.rs:370-448` (`get_cached_message`, `get_message`)
- Test: in-module test for the key helper

**Interfaces:**
- Consumes: Task 1 API.
- Produces: `fn display_key(folder_id: &str, server_uid: u32) -> String` → `format!("{folder_id}:{server_uid}")` (exported from `display_lru.rs` for tests). Invalidation helper `fn invalidate_display_lru(state: &AppState, folder_id: &str, server_uid: u32)`.

- [ ] **Step 1: Write failing test** — `display_key_formats_pair`: `display_key("f1", 7) == "f1:7"`.
- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p origami-app display_key`
Expected: FAIL

- [ ] **Step 3: Implement key helper; wire `get_cached_message` and `get_message`**

Both commands: LRU `get` first (return DTO built from cached `ParsedMessage` via the existing `message_dto` path); on miss use `parsed_with_cache` around the existing `parsed_message_for_logical_message` load. `get_message` inserts into LRU after any successful parse (display-cache hit, blob parse, or `ensure_body` path). Call `invalidate_display_lru` wherever the codebase already drops parsed state (message delete/move, account removal — find the existing invalidation sites and add one line each).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS (new test + all existing)

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-app/src/lib.rs crates/origami-app/src/commands.rs crates/origami-app/src/display_lru.rs
git commit -m "feat: serve open path from display LRU"
```

### Task 3: `PrefetchQueue` with priorities, dedupe, cancel, backoff

**Files:**
- Create: `crates/origami-core/src/prefetch_queue.rs` (declare in `lib.rs`)
- Test: in-module `#[cfg(test)]` tests (tokio current-thread)

**Interfaces:**
- Consumes: nothing new (async fetch closure injected).
- Produces:
  - `pub enum PrefetchPriority { Open = 0, Predictive = 1, Viewport = 2, Background = 3 }`
  - `pub struct PrefetchRequest { pub key: String, pub priority: PrefetchPriority }`
  - `pub struct PrefetchQueue`; `pub fn new(max_concurrency: usize) -> Self` (call sites use `3`)
  - `pub fn enqueue(&self, req: PrefetchRequest) -> bool` (`false` if key already queued/in-flight)
  - `pub fn cancel_all(&self)` (drops queued work; in-flight tasks finish but results are discarded — fetch side must check a generation counter before any store write)
  - `pub fn set_paused(&self, paused: bool)` (open-in-flight pause)
  - `pub fn set_enabled(&self, enabled: bool)` (offline / account-error / throttle kill-switch)
  - `pub fn note_failure(&self) -> bool` / `pub fn note_success(&self)` (exponential backoff state; `note_failure` returns `false` when prefetch should be disabled: 5 consecutive failures)
  - `pub fn run<F, Fut>(&self, fetch: F)` spawning workers where `F: Fn(String) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<(), String>> + Send`
  - Worker order: lowest `PrefetchPriority` discriminant first; FIFO within a priority.

- [ ] **Step 1: Write failing tests** — `open_overtakes_prefetch` (enqueue Viewport×3 then Open×1 with a fetch that records order: Open runs before remaining Viewport), `queue_dedupes_same_message` (enqueue same key twice: `false` second time; fetch called once), `cancel_stops_writes` (fetch slow; `cancel_all` mid-flight: fetch completion writes nothing — verified via generation counter check inside the test's fetch closure), `paused_defers_work` (`set_paused(true)`: nothing runs until unpause), `disabled_ignores_enqueue` (`set_enabled(false)`: `enqueue` returns `false`, no fetch), `throttle_backoff_disables_prefetch` (5× `note_failure`: returns `false` on the 5th and `enqueue` stays disabled until `note_success` path via `set_enabled(true)`).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core prefetch_queue`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `PrefetchQueue`**

Bounded worker pool (`tokio::sync::Semaphore` + `BinaryHeap` keyed by priority as a `Mutex<VecDeque<PrefetchRequest>>` per priority bucket is fine). Generation counter (`AtomicU64`) bumped by `cancel_all`; the fetch wrapper checks it after await before writing. Backoff: none needed between successes; `note_failure` counts consecutive failures (reset by `note_success`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p origami-core prefetch_queue`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/prefetch_queue.rs crates/origami-core/src/lib.rs
git commit -m "feat: priority prefetch queue with cancel and backoff"
```

### Task 4: `prefetch_display` command + queue wiring into `SyncEngine`

**Files:**
- Modify: `crates/origami-core/src/sync.rs` (hold `PrefetchQueue`; `spawn_recent_prefetch` enqueues `PrefetchPriority::Background`; existing `prefetch_recent` work moves behind queue keys)
- Modify: `crates/origami-app/src/commands.rs` (new command)
- Modify: `crates/origami-app/src/lib.rs` (register command)
- Modify: `ui/src/lib/api.ts` (wrapper)

**Interfaces:**
- Consumes: Task 3 `PrefetchQueue` API; existing `fetch_display_message`, `cache_display_message_sources`.
- Produces:
  - Tauri command `pub async fn prefetch_display(state: State<'_, AppState>, requests: Vec<PrefetchRequestDto>) -> CmdResult<()>` with `#[derive(serde::Serialize, serde::Deserialize)] pub struct PrefetchRequestDto { pub folder_id: String, pub server_uid: u32, pub priority: String }` accepting `"open" | "predictive" | "viewport"`.
  - `api.prefetchDisplay(requests: { folderId: string; serverUid: number; priority: "open" | "predictive" | "viewport" }[]) => Promise<void>`.
  - Queue fetch closure: resolve folder → `backend.fetch_display_message` → `cache_display_message_sources` for all physical sources (reuse `group_prefetch_candidates` grouping) → `note_success`; errors → `note_failure`; never touches flags.

- [ ] **Step 1: Write failing test** — extend `crates/origami-core/tests/prefetch_runtime.rs`: `prefetch_display_caches_without_flags` (existing harness: enqueue display fetch for one message; assert display cache populated and `flags_json` unchanged).
- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p origami-core --test prefetch_runtime prefetch_display_caches_without_flags`
Expected: FAIL (no API)

- [ ] **Step 3: Implement command + wiring**

Map DTO priority strings to `PrefetchPriority`; unknown string → error. Dedupe against display cache before enqueue (skip keys already cached — cheap store check `display_cache_has(folder_id, server_uid)`; add to store if absent).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src crates/origami-app/src ui/src/lib/api.ts
git commit -m "feat: prefetch_display command over priority queue"
```

### Task 5: Predictive prefetch (hover + keyboard)

**Files:**
- Create: `ui/src/lib/prefetch.ts`
- Modify: `ui/src/lib/ThreadList.svelte` (row hover + active-row change)
- Modify: `ui/src/lib/stores.svelte.ts` (enqueue on selection move)
- Test: `ui/src/lib/prefetch.test.ts`

**Interfaces:**
- Consumes: `api.prefetchDisplay` (Task 4).
- Produces: `export function createPrefetcher(opts?: { debounceMs?: number })` returning `{ hover(envelope: Envelope): void; focusMove(envelope: Envelope | null): void; cancel(): void }`. Default `debounceMs = 150`. For an envelope it enqueues the primary source `{ folderId, serverUid, priority: "predictive" }` via `api.prefetchDisplay` (batch whatever accumulated in the debounce window). `cancel()` drops the pending timer (used on folder switch).

- [ ] **Step 1: Write failing tests** — `debounces_sweeps` (3 `hover` calls within 150ms with fake timers: exactly one `prefetchDisplay` call), `sends_predictive_priority` (payload priority is `"predictive"`), `cancel_drops_pending` (hover then `cancel()`: no `prefetchDisplay` after timers run).
- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run ui/src/lib/prefetch.test.ts`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement `prefetch.ts` + wire hover/focusMove**

`ThreadList` row `onmouseenter` → `prefetcher.hover(envelope)`. `selectEnvelope` success → `prefetcher.focusMove(envelope)`. Folder switch path (wherever envelopes are replaced) → `prefetcher.cancel()`. Mock `api.prefetchDisplay` in tests (`vi.mock`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add ui/src/lib/prefetch.ts ui/src/lib/prefetch.test.ts ui/src/lib/ThreadList.svelte ui/src/lib/stores.svelte.ts
git commit -m "feat: predictive prefetch on hover and keyboard"
```

### Task 6: Viewport prefetch

**Files:**
- Modify: `ui/src/lib/prefetch.ts` (extend `createPrefetcher`)
- Modify: `ui/src/lib/ThreadList.svelte` (observe `VirtualList` visible range)
- Modify: `ui/src/lib/VirtualList.svelte` (expose visible index range)
- Test: `ui/src/lib/prefetch.test.ts` (extend)

**Interfaces:**
- Consumes: Task 5 `createPrefetcher`, Task 4 `api.prefetchDisplay`.
- Produces: `createPrefetcher` return gains `viewport(envelopes: Envelope[], visible: { start: number; end: number }): void` — enqueues rows in `[start - (end - start), end + (end - start)]` clamped to array bounds (one neighbor viewport each side), priority `"viewport"`, debounced 200ms, batched into one `prefetchDisplay` call. `VirtualList` emits/accepts a `visibleRange` callback `(range: { start: number; end: number }) => void` from its scroll math.

- [ ] **Step 1: Write failing tests** — `viewport_enqueues_margin` (20 rows, visible 5–10: enqueues indices 0–20 rows in margin, one call), `viewport_clamps_bounds` (visible 0–2: no negative indices), `viewport_not_spammy` (5 rapid scroll callbacks: one `prefetchDisplay`).
- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run ui/src/lib/prefetch.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement viewport batching + `visibleRange` in `VirtualList`**

Feed `viewport(app.value.envelopes, range)` from ThreadList whenever `VirtualList` reports a new visible range or the envelope array is replaced (folder load).

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add ui/src/lib/prefetch.ts ui/src/lib/prefetch.test.ts ui/src/lib/ThreadList.svelte ui/src/lib/VirtualList.svelte
git commit -m "feat: viewport prefetch with neighbor margin"
```

### Task 7: Instant header paint

**Files:**
- Modify: `ui/src/lib/MessageView.svelte` (header block + loading state)
- Test: `ui/src/lib/MessageView.test.ts` (extend)

**Interfaces:**
- Consumes: `app.value.selectedEnvelope` (list DTO already in hand).
- Produces: while `messageLoading` and `selectedEnvelope` is set, the header (subject, sender, date, snippet if present on the envelope DTO) renders from `selectedEnvelope`; the skeleton renders **only** in the body region. When `message` arrives, header values switch to the message DTO (richer addresses/details).

- [ ] **Step 1: Write failing tests** — `header_paints_from_envelope_while_loading` (render with `messageLoading: true`, `selectedEnvelope` set, `message: null`: subject text visible, skeleton present), `skeleton_confined_to_body` (skeleton node is not inside the header element), `header_switches_to_message_when_loaded` (same envelope + message DTO with different subject detail: detail visible).
- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run ui/src/lib/MessageView.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement header/body split in `MessageView.svelte`**

Bind header fields to `app.value.message?.envelope ?? app.value.selectedEnvelope`. Keep `role="status"` / `aria-live="polite"` on the body skeleton only.

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add ui/src/lib/MessageView.svelte ui/src/lib/MessageView.test.ts
git commit -m "feat: paint message header from envelope while body loads"
```

### Task 8: Display-cache eviction

**Files:**
- Modify: `crates/origami-core/src/store.rs` (new method near `recent_uncached_messages`)
- Modify: `crates/origami-core/src/sync.rs` (call after prefetch batch completes)
- Test: `crates/origami-core/tests/store.rs` (extend)

**Interfaces:**
- Consumes: existing display-cache tables (`cache_display_message_sources` writes).
- Produces: `pub fn evict_display_cache(&self, max_rows: u32, max_age_days: u32) -> Result<u32>` — deletes display-cache rows older than `max_age_days` (by message received time), then oldest-first beyond `max_rows`; returns rows deleted. Call sites use `(2000, 30)`.

- [ ] **Step 1: Write failing tests** — `evict_display_cache_age` (insert old + recent rows: only recent survive `max_age_days`), `evict_display_cache_count` (insert 3 rows, `max_rows = 2`: oldest received evicted first, returns 1).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test store evict_display_cache`
Expected: FAIL

- [ ] **Step 3: Implement `evict_display_cache` + call after prefetch batches**

Only display-derived parsed rows are evicted; raw body blobs and FTS rows for fetched bodies are untouched (delete the parsed-display cache representation only — check how `cache_display_message_sources` marks display-only rows vs body-backed rows and preserve body-backed ones).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/store.rs crates/origami-core/src/sync.rs crates/origami-core/tests/store.rs
git commit -m "feat: bound display cache by age and row count"
```

### Task 9: Offline / error-state guard + cancel-on-navigation wiring

**Files:**
- Modify: `crates/origami-app/src/commands.rs` (`prefetch_display` guards) and/or `crates/origami-core/src/sync.rs` (account status → `set_enabled`)
- Modify: `ui/src/lib/stores.svelte.ts` (cancel prefetcher on folder switch — Task 5 touched the same path; this task covers message navigation and account removal)
- Test: `crates/origami-core/tests/prefetch_runtime.rs` + `ui/src/lib/prefetch.test.ts` (extend)

**Interfaces:**
- Consumes: Tasks 3–6.
- Produces: behavior — `prefetch_display` is a no-op (returns `Ok(())`) when the resolved account has no backend/online state or is in error state; `selectEnvelope` switching to a different message cancels pending predictive work for the previous row (via `prefetcher.cancel()` then re-enqueue policy as in Task 5); `remove_account` cancels all queue work for that account.

- [ ] **Step 1: Write failing tests** — `prefetch_noop_when_account_error` (harness account in error state: `prefetch_display` succeeds, no fetch occurs), `cancel_on_selection_change` (UI: hover row A, select row B quickly: only B's flow fetches).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test prefetch_runtime prefetch_noop_when_account_error` and `npx vitest run ui/src/lib/prefetch.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement guards**

Thread online/error state into `PrefetchQueue::set_enabled` at the places account status changes (reuse `account_statuses` computation rather than duplicating state).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked && npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-app/src crates/origami-core/src crates/origami-core/tests ui/src/lib
git commit -m "feat: guard prefetch behind account health and navigation"
```

### Task 10: Cached-open latency smoke (measurement check)

**Files:**
- Create: `crates/origami-app/tests/open_latency.rs` (or in-module if harness-only)
- Test: the file itself

**Interfaces:**
- Consumes: Task 1–2 API.
- Produces: `#[ignore]`d test `cached_open_under_budget` — warms LRU with one `ParsedMessage`, times 1,000 `parsed_with_cache` hits, asserts mean < 50ms (generous CI-safe bound 50ms even though real hits are µs; run locally with `cargo test -- --ignored`). Documented as the spec's "cached-open budget" check.

- [ ] **Step 1: Write the timing test** (as above).
- [ ] **Step 2: Run it explicitly**

Run: `cargo test -p origami-app --test open_latency -- --ignored`
Expected: PASS with mean well under 50ms.

- [ ] **Step 3: Run full suites**

Run: `cargo test --workspace --locked && npm test --prefix ui`
Expected: PASS

- [ ] **Step 4: Commit** (if authorized)

```bash
git add crates/origami-app/tests/open_latency.rs
git commit -m "test: cached-open latency budget check"
```
