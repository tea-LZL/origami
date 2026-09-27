# Origami — Hardening, Open-Speed Caching, Palette Design

Date: 2026-09-26
Status: approved design, pending implementation plans
Scope: three workstreams — (A) open-speed caching, (B) hardening, (C) palette

## Goals

1. **Open speed** — opening an email paints content as fast as possible: cached opens
   effectively instant, cold opens never slower than today, and the common "walk the
   list" reading flow stays cache-warm.
2. **Hardening** — security and data safety (permissions, secrets, hostile input,
   protocol scoping) plus runtime robustness (sync failure isolation, outbox
   correctness, crash safety, no panics on network paths).
3. **Palette** — richer, warmer color system so the app reads less gloomy and sterile,
   without breaking accessibility or the provider-neutral UI rule.

## Non-goals

- No new background daemon or service. Everything runs in the existing process
  ([[runtime-and-layers]]).
- No provider-specific UI or backend abstractions.
- No broad component rewrites (per [[release-readiness]] "Recommended QoL order").
- Proof/release gates (Playwright suites, tag publish path, license files) are out of
  scope here; only per-fix regression checks, per repo rule.
- PGP, JMAP, new backends — unchanged, deferred.

## Baseline (as-built facts)

- Open path today: select row → skeleton → `get display or fallback to ensure_body`
  (`crates/origami-app/src/commands.rs`) → render. Envelope header waits with body.
- Display cache: normalized display MIME cached per physical source
  (`cache_display_message_sources`), warmed by `spawn_recent_prefetch` /
  `prefetch_recent` over a 7-day window, `PREFETCH_MAX_MESSAGES = 500`,
  `PREFETCH_MAX_MESSAGE_SIZE = 8 MiB`, deduped by `group_prefetch_candidates`,
  serialized by `prefetch_locks`.
- Bodies: raw RFC 822 as content-addressed blobs; FTS indexed after body fetch.
- Stale-response rejection and coalesced folder refreshes already exist.
- Themes: `system | light | dark` (`ui/src/lib/stores.svelte.ts`), values duplicated
  between `:root[data-theme="dark"]` and the `prefers-color-scheme` block in
  `ui/src/app.css`.
- Tag palette: 6 hues (`TAG_PALETTE` in `ui/src/lib/tags.ts`).
- File perms: `private_fs.rs` enforces `0700` roots / `0600` files on open and write;
  existing-data native smoke still outstanding ([[release-readiness]]).

---

## Track A — Open-speed caching

Ship in slices, each independently valuable:

### A1. In-memory display LRU (pure win)

- New LRU in `origami-app` state: key = `"{folder_id}:{server_uid}"` (matches the
  open-path command signatures; logical dedupe already happens upstream), value =
  parsed display DTO.
- **Byte-budget cap (32 MiB) + count cap (~300)**; evict least-recently-used.
- Open read order: LRU → SQLite display cache → network display fetch →
  `ensure_body` fallback (unchanged semantics).
- Re-open cost drops from "SQLite read + MIME parse" to "map lookup". Parse happens
  once per message per process lifetime.

### A2. Predictive prefetch (hover + keyboard)

- UI: row hover (debounce ~150 ms) and keyboard next/prev focus enqueue
  `prefetch_display(message_ids)` for the row under consideration.
- Same backend queue as A3, priority **above** viewport batch.
- Cheap (~50 LOC of UI), covers sequential reading ("next, next, next").

### A3. Viewport prefetch (last, most overhead)

- UI: on folder load and on scroll (debounced), enqueue display fetches for visible
  rows + one viewport of neighbor margin (both directions).
- Backend batches via existing `prefetch_locks`, **concurrency 2–3**, in-flight dedupe,
  never touches `Seen` or any flags.
- Existing recent-N warm (`spawn_recent_prefetch`) stays as the tail coverage and must
  dedupe against viewport requests.

### A4. Priority queue + cancellation

Single request queue behind `prefetch_display` / open path, priorities:

1. user open (preempts everything; prefetch pauses while an open fetch is in flight)
2. hover / keyboard predictive
3. viewport batch
4. background recent-N warm

- Cancel queued work on navigation away, folder switch, account removal.
- All prefetch stops when offline or when the account is in error state.

### A5. Instant paint + progressive upgrade

- Envelope header (subject, sender, date, snippet) renders immediately from the list
  DTO already in hand — skeleton remains **only** in the body region.
- Display-cache hit → body paints with no network wait.
- Full body / attachments / remote-image enablement upgrade the view progressively;
  none of them block the first paint.

### A6. Eviction and budget discipline

- Display-cache rows: evict oldest-`received` beyond a count/days cap (write-once-ish
  data; LRU-touch churn is wasteful).
- Blobs: raw bodies retained as today; attachments stay on demand.
- Network budget: batched small fetches; on throttle signals (`*BYE`, quota errors)
  back off and stop prefetch after repeated failures.

### Performance and resource budget (accepted trade-offs)

| Resource | Cost | Notes |
|---|---|---|
| Memory | +5–15 MiB typical, 32 MiB cap | vs webview baseline this is noise |
| CPU | burst of MIME parses on folder open (background thread); **lower** on re-open | never on UI thread |
| Network | ~1–2 MiB display MIME per first visit of a folder view | text parts only; zero after warm |
| Disk | ~1–2 MB WAL churn per new folder view | mild checkpoint pressure |
| Open latency | **improved** if priority queue holds | hazard: prefetch saturating the IMAP link; mitigated by A4 |
| Complexity | +500–800 LOC, concurrency surface | interacts with B5 lock audit; needs queue/cancel/backoff tests |

**Main hazard**: prefetch traffic delaying a user open. Mitigation is A4's strict
priority + pause-during-open, plus throttle backoff. Slice order (A1 → A2 → A3) means
the risky piece ships only after the cheap wins are proven.

### Verification (Track A)

- Cached-open budget: body paints < 50 ms from LRU hit, < 100 ms from SQLite hit
  (measured in a store-level timing regression check).
- Queue ordering test: open request overtakes queued prefetch.
- Cancel test: folder switch drops queued prefetch; no writes after cancel.
- Throttle test: repeated fetch failures disable prefetch, leave open path healthy.
- Eviction test: caps honored, oldest-received evicted first.

---

## Track B — Hardening

### B1. Security and data safety

1. **Existing-data permission smoke** (release-readiness blocker): fixture with
   `0755` dirs / `0644` files → open app path → assert `0700` roots and `0600`
   config/database/new blobs.
2. **Dependency policy**: `cargo-audit` + `cargo-deny` in CI (advisories fail the
   build; license set pinned to the declared MIT/Apache-2.0); npm audit step pinned
   (tree is currently clean).
3. **Secret redaction audit**: keyring material must never appear in logs, error
   chains, or technical-detail DTOs. Regression check asserting redaction.
4. **Hostile-MIME corpus**: deeply nested multiparts, huge/overlong headers,
   header-injection attempts, `cid:` bombs, malformed base64, oversized attachment
   declarations → parse/display must not panic or allocate unbounded. Attachment
   decode caps enforced at the blob/decode layer.
5. **cid: protocol scope proof**: custom protocol serving must be scoped per message;
   test proves no cross-message blob read and no path traversal.
6. **Link scheme tests**: `javascript:`, `file:`, `data:` and friends rejected by the
   link interceptor (system-handler allowlist stays HTTP/HTTPS/mailto).

### B2. Runtime robustness

1. **Sync failure isolation**: one folder failure never kills account sync; retry with
   backoff + jitter; IDLE reconnect loop capped; reconnect-storm guard.
2. **Outbox correctness**: idempotent replay (safe after crash mid-retry); poison
   messages move to a terminal `failed` state surfaced in the Outbox UI — no infinite
   retry. Existing duplicate-send protection retained.
3. **Crash safety**: audit that every multi-row write is transactional; drafts flush on
   quit; graceful shutdown finishes or cancels in-flight sync and checkpoints WAL.
4. **Network-path panic audit**: timeouts on every IMAP/SMTP op; transient vs
   permanent error classification; `unwrap`/`expect` removed from network paths
   (current count in `sync.rs` is small but non-zero).
5. **Lock hygiene**: `body_locks` / `prefetch_locks` / new A4 queue — deadlock audit,
   bounded lock waits, no lock held across `.await` on a network call.
6. **Store invariants**: foreign keys on; dedupe/physical-source retention invariants
   get assertion-level regression checks.

### Verification (Track B)

One runnable regression check per fix (repo rule), including: perm smoke, redaction
assertion, hostile-MIME corpus run, cid scope test, scheme-rejection tests, outbox
poison test, folder-failure isolation test, shutdown-flush test.

---

## Track C — Palette

### C1. Token layer

- One semantic token set in `:root` of `ui/src/app.css`: surfaces, text, borders,
  accent scale, plus **status hues** — success, warning, danger, info.
- Themes map tokens → values only. The duplicated dark block
  (`[data-theme="dark"]` vs `prefers-color-scheme`) collapses to one mapping applied
  by both selectors.

### C2. Semantic color usage

- Folder-role icons carry hue: Inbox blue, Drafts amber, Sent teal, Junk rose,
  Trash/Archive muted violet/gray.
- Outbox rows and account sync status chips use status hues (pending, failed,
  syncing, ok). No status conveyed by color alone — icon or label always present.

### C3. Tag palette

- `TAG_PALETTE` grows from 6 to ~12 hues (`ui/src/lib/tags.ts`), each contrast-checked
  against both theme backgrounds; hash distribution stays uniform.

### C4. New warm dark theme — "ember"

- Warm charcoal/ink surfaces + amber accent family: kills the navy gloom without
  leaving dark mode.
- Theme union becomes `"system" | "light" | "dark" | "ember"`; Preferences picker
  lists it; `loadPreferences` accepts it; forced-colors paths preserved untouched.

### Verification (Track C)

- Contrast check (WCAG AA) for fg/bg token pairs in every theme — small runnable
  script over the token table.
- `forced-colors` behavior unchanged (existing pip/tint rules still clear correctly).
- Theme preference round-trip test (`loadPreferences` / `savePreferences`).

---

## Delivery order

1. **B1 security gates first** — small, unblock release readiness (perm smoke,
   cargo-audit/deny, redaction).
2. **A1 → A2 → A3** — caching slices, cheap wins first, risky piece last, each with
   its regression checks.
3. **C anytime** — independent, no cross-track coupling; C1 token layer before C4.
4. **B2 robustness** interleaved with A (A4 queue work shares the lock-hygiene audit).

## Cross-cutting rules

- UI stays provider-neutral ([[backend-seam]]); prefetch and queue logic live in
  `origami-core` / `origami-app`, not in Svelte.
- Root-cause fixes in Rust/store, not Svelte-side reconstruction.
- One runnable regression check per fix.
- Unrelated dirty-worktree changes preserved; no commits unless explicitly requested.

## Decisions

- **LRU owner**: `origami-app` `AppState` — process-lifetime scope, keeps
  `origami-core` UI-agnostic and store-focused.
- **Default caps** (tunable from measurement during A6): display LRU 32 MiB / ~300
  entries; display-cache rows 2,000 or 30 days received-age, whichever binds first.
- **Implementation plans**: one per track (A, B, C), written after this spec is
  approved — a single plan for all three tracks would be too coarse.

## Open questions (non-blocking)

- "ember" theme name — renameable at any point.
