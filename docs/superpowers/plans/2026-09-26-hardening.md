# Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the security/data-safety gaps (permissions, dependency policy, secret redaction, hostile MIME, blob/attachment scoping) and the runtime-robustness gaps (sync failure isolation, outbox poison handling, crash safety, panic/timeout hygiene, lock hygiene).

**Architecture:** Most work is defense-in-depth at existing boundaries: `private_fs` perm enforcement gets an existing-data smoke, the store gets transactional/eviction-safe write paths and an outbox terminal-failure state, `sync.rs` gets classified errors + capped retries, and the sanitizer/attachment edges get corpus tests. No new subsystems.

**Tech Stack:** Rust (rusqlite, tokio, mail-parser), cargo-audit/cargo-deny, Tauri 2 commands, Svelte 5 + vitest.

**Spec:** `docs/superpowers/specs/2026-09-26-hardening-caching-palette-design.md` (Track B)

## Global Constraints

- Root-cause fixes in Rust/store; no Svelte-side data reconstruction.
- One runnable regression check per fix.
- Offline mutation semantics (outbox replay, duplicate-send protection) must not regress ([[offline-outbox]]).
- UI stays provider-neutral; error copy must not leak provider specifics outside onboarding.
- Commits only when the executor is explicitly authorized to commit; otherwise leave the worktree dirty.

## Review Focus

- Pre-existing `0755`/`0644` data dirs and files must end up `0700`/`0600` after one open — without deleting user data (`existing_data_perms_tightened`).
- A message whose raw MIME declares a 4 GiB attachment must not cause unbounded allocation during parse or display (`attachment_size_cap_enforced`).
- A blob hash or folder id containing `../` must never resolve outside `blobs/<aa>/<hash>` (`blob_path_rejects_traversal`).
- An outbox op that fails permanently (e.g. invalid recipient rejected by SMTP) must stop retrying and surface as failed, not loop forever (`outbox_poison_goes_terminal`).
- One folder failing IMAP must leave other folders and the account status recovering normally (`folder_failure_isolated`).

---

### Task 1: Existing-data permission smoke

**Files:**
- Test: `crates/origami-core/tests/store.rs` (extend; use `private_fs`-equivalent open path)
- Modify: `crates/origami-core/src/private_fs.rs` only if the smoke finds enforcement gaps

**Interfaces:**
- Consumes: `private_fs::{create_private_dir, secure_existing_file, write_private}` (crate-visible; test reaches them through the store/config open path they guard).
- Produces: test `existing_data_perms_tightened` in `tests/store.rs`.

- [ ] **Step 1: Write failing test** — `existing_data_perms_tightened`: tempdir fixture with `db/` at `0755`, `db.sqlite3` at `0644`, `config` at `0644`, one `blobs/aa/hash` file at `0644`; open the store the way `origami-app` does (the public store-open constructor); assert `metadata.permissions().mode() & 0o777` equals `0o700` for roots and `0o600` for config/database/blob files; assert file contents unchanged (byte compare before/after).
- [ ] **Step 2: Run test to verify it fails** (or fails to run)

Run: `cargo test -p origami-core --test store existing_data_perms_tightened`
Expected: FAIL if enforcement misses pre-existing files; if it passes, the smoke still lands as the release-readiness evidence.

- [ ] **Step 3: Close any gap the smoke exposes**

Tighten at the open path only (call `secure_existing_file` on known paths at open). No new file formats.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/tests/store.rs crates/origami-core/src/private_fs.rs
git commit -m "test: existing-data permission tightening smoke"
```

### Task 2: cargo-audit + cargo-deny policy in CI

**Files:**
- Create: `deny.toml` (workspace root)
- Modify: `.github/workflows/ci.yml` (rust job, after `test` step)
- Test: CI step itself (local run is the check)

**Interfaces:**
- Consumes: none.
- Produces: `deny.toml` with `[advisories]` vulnerability = "deny", `[licenses]` allow = ["MIT", "Apache-2.0", "Unicode-DFS-2016", "Unicode-3.0", "BSD-3-Clause", "ISC", "Zlib", "MPL-2.0"] (trim to what `cargo deny` reports as actually used — the app declares MIT/Apache-2.0; adjust the list only with a code comment explaining the exception); `[bans]` multiple-versions = "warn". CI steps: `cargo install cargo-audit cargo-deny --locked` (or `taiki-e/install-action`), then `cargo audit` and `cargo deny check`.

- [ ] **Step 1: Add `deny.toml`, run locally**

Run: `cargo deny check`
Expected: either PASS or a concrete advisory/license list to resolve.

- [ ] **Step 2: Resolve findings** (update or patch offending crates; no `unwrap`-level suppressions without a comment + tracking note in `docs/wiki/status/known-drift.md`).

- [ ] **Step 3: Add CI steps** after the existing `test` step in the rust job of `.github/workflows/ci.yml`, mirroring the existing Arch-container setup.

- [ ] **Step 4: Verify CI passes on a push** (or `act`/dry reasoning with the workflow diff reviewed).

- [ ] **Step 5: Commit** (if authorized)

```bash
git add deny.toml .github/workflows/ci.yml
git commit -m "ci: enforce cargo-audit and cargo-deny policy"
```

### Task 3: Secret redaction

**Files:**
- Create: `crates/origami-core/src/redact.rs` (declared in `lib.rs`)
- Test: in-module tests
- Modify: `crates/origami-app/src/commands.rs` error mapping (`fn err`) if the audit finds leak paths

**Interfaces:**
- Consumes: none.
- Produces: `pub fn redact_secrets(input: &str) -> String` replacing any substring matching keyring/password material patterns: `password=...`, `PASS ...`, `Authorization: ...`, `oauthAccessToken`/`refresh_token` JSON values, and any value previously loaded from the keyring (caller passes known secrets: `pub fn redact_with(input: &str, secrets: &[&str]) -> String` masks each secret occurrence with `[redacted]`).

- [ ] **Step 1: Write failing tests** — `redacts_known_secret` (`redact_with("token abc123 ok", &["abc123"]) == "token [redacted] ok"`), `redacts_auth_headers`, `redacts_json_tokens`, `no_false_positive_on_short_strings` (empty secret list: input unchanged).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core redact`
Expected: FAIL

- [ ] **Step 3: Implement `redact.rs`; audit `commands.rs` + `sync.rs` error strings**

Apply `redact_with` in the `err` mapping used for `CmdResult` errors and in `OutboxEntry.last_error` writes (secrets list = the account password/token currently held in config for that operation). Regression test: `outbox_last_error_redacted` in `crates/origami-core/tests/store.rs` (add an outbox entry whose error string embeds the password, assert stored/displayed form is redacted).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/redact.rs crates/origami-core/src/lib.rs crates/origami-core/src/store.rs crates/origami-core/tests/store.rs crates/origami-app/src/commands.rs
git commit -m "fix: redact secrets from errors and outbox rows"
```

### Task 4: Hostile-MIME corpus + attachment decode caps

**Files:**
- Create: `crates/origami-core/tests/hostile_mime.rs` + fixture files under `crates/origami-core/tests/fixtures/hostile/`
- Modify: `crates/origami-core/src/message.rs` (caps) and/or `crates/origami-core/src/blob.rs` (decode caps)
- Modify: `crates/origami-core/src/sync.rs` (apply `PREFETCH_MAX_MESSAGE_SIZE`-style cap to display fetch size if not already enforced)

**Interfaces:**
- Consumes: `origami_core::message::parse`, `parse_display`, `DisplayMessage` fetching path.
- Produces: consts `pub const MAX_MIME_PART_DEPTH: u32 = 32`, `pub const MAX_HEADER_BYTES: usize = 64 * 1024`, `pub const MAX_ATTACH_DECODE_BYTES: usize = 64 * 1024 * 1024` in `message.rs`; parse returns `Err` or truncated-safe `ParsedMessage` with `parse_warnings` entries when caps trip — never panics, never unbounded alloc.

- [ ] **Step 1: Write failing corpus tests** — one test per fixture, each asserting `parse(&raw).is_ok()` with warnings OR a clean `Err`, and a wall-clock/alloc sanity bound by size of output vs input:
  - `deep_multiparts` (1,000 nested `multipart/mixed`)
  - `huge_headers` (single header 1 MiB)
  - `header_injection` (embedded CRLF sequences in From/Subject)
  - `cid_bomb` (10k `cid:` refs in HTML)
  - `malformed_base64` (truncated/absurd base64 in an attachment part)
  - `attachment_size_lie` (declared 4 GiB attachment, tiny actual body)
  Add `parse_is_total` smoke: every fixture returns within the caps without panic (catch_unwind wrapper acceptable in the test).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test hostile_mime`
Expected: FAIL (fixtures/caps absent)

- [ ] **Step 3: Implement caps in `message.rs` / `blob.rs`**

Enforce depth and header caps in the MIME walker; attachment decode streams with `MAX_ATTACH_DECODE_BYTES` cutoff; declared sizes never trusted for allocation. Keep `parse_warnings` populated so the UI can show "large content omitted".

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/tests/hostile_mime.rs crates/origami-core/tests/fixtures/hostile crates/origami-core/src/message.rs crates/origami-core/src/blob.rs
git commit -m "fix: harden MIME parsing with caps and hostile corpus"
```

### Task 5: Blob/attachment access scoping

**Files:**
- Test: `crates/origami-core/tests/store.rs` (extend) and `crates/origami-app/src/commands.rs` test module
- Modify: `crates/origami-core/src/blob.rs` (path validation) and `crates/origami-app/src/commands.rs` (`get_attachment`) only if tests expose gaps
- Test: `ui/src/lib/messageHtml.test.ts` (extend) for inert `cid:` handling

**Interfaces:**
- Consumes: `blobs().get(&hash)`, `get_attachment(folder_id, server_uid, index)`, `ui/src/lib/messageHtml.ts` sanitizer.
- Produces: `fn blob_relative_path(hash: &str) -> Result<String>` in `blob.rs` — rejects hashes outside `[a-f0-9]{64}` (or whatever the content-address format is — match `blob.rs` hashing) before path join.

- [ ] **Step 1: Write failing tests** — `blob_path_rejects_traversal` (`blobs().get("../etc/passwd")` and `get("aa/../../x")` → `Err`, no fs touch outside root), `get_attachment_rejects_wrong_message` (attachment index of message A requested with message B's `(folder, uid)` → `Err`), `get_attachment_rejects_bad_index` (index 999 → `Err`), `cid_images_stay_inert` (sanitizer keeps `cid:` `img[src]` as-is: not rewritten to `http:`, `file:`, or `data:`; `file://` src stripped).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test store blob_path` and `cargo test -p origami-app` and `npx vitest run ui/src/lib/messageHtml.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement validation** at the blob lookup and attachment lookup boundaries.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked && npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/blob.rs crates/origami-core/tests/store.rs crates/origami-app/src ui/src/lib/messageHtml.test.ts
git commit -m "fix: scope blob and attachment access to owning message"
```

### Task 6: Link scheme rejection tests

**Files:**
- Test: `ui/src/lib/messageHtml.test.ts` and `ui/src/lib/MessageView.test.ts` (extend)
- Modify: `ui/src/lib/messageHtml.ts` / `MessageView.svelte` only if gaps found

**Interfaces:**
- Consumes: existing link interceptor (`MessageView.svelte:91` allowlist `http:`, `https:`, `mailto:`).
- Produces: tests only (behavior already implemented).

- [ ] **Step 1: Write failing tests if gaps exist** — `rejects_javascript_links`, `rejects_file_links`, `rejects_data_links`, `rejects_vbscript_and_custom_schemes` (each as sanitizer output: `href` stripped; and as interceptor: `Unsupported link type` thrown). Case-sensitivity and whitespace tricks (`JaVaScRiPt:`, `java\tscript:`) included.
- [ ] **Step 2: Run tests**

Run: `npx vitest run ui/src/lib/messageHtml.test.ts ui/src/lib/MessageView.test.ts`
Expected: PASS where behavior exists; FAIL where a trick slips through.

- [ ] **Step 3: Fix any gaps** at the sanitizer/interceptor boundary (normalize scheme before allowlist check).

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add ui/src/lib/messageHtml.ts ui/src/lib/MessageView.svelte ui/src/lib/messageHtml.test.ts ui/src/lib/MessageView.test.ts
git commit -m "test: hostile link scheme rejection"
```

### Task 7: Sync failure isolation + capped retries

**Files:**
- Modify: `crates/origami-core/src/sync.rs` (per-folder sync path, IDLE loop)
- Test: `crates/origami-core/tests/sync_status.rs` or `sync_harness.rs` (extend)

**Interfaces:**
- Consumes: existing sync harness (`sync_harness.rs`), `Error` classification (extend enum or map: `fn is_transient(err: &Error) -> bool` in `sync.rs`).
- Produces: behavior — one folder's IMAP failure leaves other folders syncing and account status `degraded`-but-alive; retries only on transient errors with exponential backoff + jitter (base 2s, cap 5min, ±20% jitter); IDLE reconnect attempts capped (e.g. 10 consecutive failures then slow-mode 60s, never a hot loop).

- [ ] **Step 1: Write failing tests** — `folder_failure_isolated` (harness: folder B fetch errors, folder A still syncs, account error cleared after later complete success — preserves existing "clear errors only after complete successful account sync" rule), `transient_error_retries_permanent_fails_fast` (mock: timeout retries, auth rejection does not), `idle_reconnect_capped` (repeated drop: reconnect spacing ≥ cap, no more than N attempts per window).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test sync_harness folder_failure_isolated` (and siblings)
Expected: FAIL

- [ ] **Step 3: Implement isolation + backoff + `is_transient`**

Loop folders independently inside account sync; classify `io` timeouts/`*BYE` as transient, auth/parse as permanent.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/sync.rs crates/origami-core/tests
git commit -m "fix: isolate folder sync failures with capped backoff"
```

### Task 8: Outbox poison → terminal failed state

**Files:**
- Modify: `crates/origami-core/src/store.rs` (migration + queries), `crates/origami-core/src/model.rs` (`OutboxEntry`)
- Modify: `crates/origami-core/src/sync.rs` (replay loop) and/or `crates/origami-app/src/commands.rs` (`retry_outbox`, `list_outbox`)
- Modify: `ui/src/lib/Outbox.svelte`, `ui/src/lib/types.ts`, `ui/src/lib/api.ts`
- Test: `crates/origami-core/tests/store.rs`, `ui/src/lib/Outbox.test.ts` (create if absent)

**Interfaces:**
- Consumes: `outbox` table (has `attempts`, `last_error`), `OutboxOp`, `OutboxEntry`.
- Produces:
  - Migration: `ALTER TABLE outbox ADD COLUMN failed_at INTEGER;` (NULL = active). Migration numbering follows the existing migration list in `store.rs`.
  - `OutboxEntry` gains `pub failed_at: Option<i64>`.
  - `store.outbox_fail_permanent(id: i64, error: &str) -> Result<()>` (sets `failed_at`, stores redacted error via Task 3).
  - Replay rule: `attempts >= 5` with permanent-classified errors (Task 7 `is_transient == false`) → `outbox_fail_permanent`; transient errors keep retrying with backoff.
  - `list_outbox` includes failed rows; `Outbox.svelte` renders them with danger-hued `Failed` chip and a `Retry` action that clears `failed_at` and resets `attempts` (`store.outbox_reopen(id)`).

- [ ] **Step 1: Write failing tests** — `outbox_poison_goes_terminal` (insert op, replay with permanent error ×1: row has `failed_at` set, `outbox_count` for pending excludes it), `outbox_reopen_resets` (`outbox_reopen`: `failed_at` NULL, `attempts` 0), UI `failed_rows_show_retry` (failed entry renders Retry control).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test store outbox_poison` and `npx vitest run ui/src/lib/Outbox.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement migration + replay rule + UI chip** (danger token already exists: `--danger`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked && npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src crates/origami-core/tests ui/src/lib/Outbox.svelte ui/src/lib/Outbox.test.ts ui/src/lib/types.ts ui/src/lib/api.ts
git commit -m "fix: outbox poison messages go terminal failed with retry"
```

### Task 9: Crash safety (transactions, drafts flush, shutdown checkpoint)

**Files:**
- Modify: `crates/origami-core/src/store.rs` (wrap multi-row writes in `tx` where missing)
- Modify: `crates/origami-app/src/lib.rs` / `state.rs` (shutdown hook) and `ui/src/lib/stores.svelte.ts` (flush drafts on quit)
- Test: `crates/origami-core/tests/store.rs` (extend)

**Interfaces:**
- Consumes: existing store write helpers, `save_composer_draft` command path.
- Produces: behavior — every multi-row write is transactional (audit + fix); app exit path calls draft flush then WAL checkpoint (`PRAGMA wal_checkpoint(TRUNCATE)` via a `store.shutdown_flush() -> Result<()>` method).

- [ ] **Step 1: Write failing tests** — `multi_row_writes_atomic` (inject failure mid-`store_flags_batch` across messages via a transaction-test hook or by asserting the existing transactional wrapper covers batch ops: partial apply never visible), `shutdown_flush_checkpoints` (write rows, `shutdown_flush()`, assert `-wal` file shrinks/checkpoints and data readable after reopen).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test store multi_row_writes_atomic`
Expected: FAIL if audit finds un-wrapped sequences.

- [ ] **Step 3: Fix write paths + add `shutdown_flush` + Tauri on-exit hook + UI quit flush** (reuse existing `save_composerDraft` invocation — flush is a forced call of it).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked && npm test --prefix ui`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/store.rs crates/origami-core/tests/store.rs crates/origami-app/src ui/src/lib/stores.svelte.ts
git commit -m "fix: transactional writes and shutdown flush"
```

### Task 10: Store invariants (foreign keys + physical-source retention)

**Files:**
- Modify: `crates/origami-core/src/store.rs` (`PRAGMA foreign_keys = ON` at open if missing; assertion helpers)
- Test: `crates/origami-core/tests/store.rs` (extend)

**Interfaces:**
- Consumes: `deduplicate_envelopes`, `merge_envelope_sources`, schema.
- Produces: `fn assert_store_invariants(conn: &Connection) -> Result<()>` (test-support or debug-build helper) checking: no orphan `messages.folder_id`, every retained logical message keeps ≥ 1 row in its physical sources, FTS row count == body-indexed message count. `foreign_keys` pragma enforced at every connection open.

- [ ] **Step 1: Write failing tests** — `foreign_keys_enforced` (insert message with bogus `folder_id` → `Err`), `physical_sources_never_orphaned` (delete one physical copy of a two-source logical message: `sources` retains the other; zero-source state impossible), `assert_store_invariants_clean_on_fixture` (seeded store passes).
- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p origami-core --test store foreign_keys_enforced`
Expected: FAIL

- [ ] **Step 3: Enable pragma at open + implement assertions + fix any orphan-producing path the tests expose.**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src/store.rs crates/origami-core/tests/store.rs
git commit -m "fix: enforce foreign keys and store invariants"
```

### Task 11: Network panic/timeout hygiene

**Files:**
- Modify: `crates/origami-core/src/sync.rs` and any `unwrap`/`expect` on network/IO paths (workspace grep: `unwrap()\|expect(` in `sync.rs`, backend adapters)
- Modify: `crates/origami-core/src/backend/` adapters (IMAP/SMTP op timeouts)
- Test: `crates/origami-core/tests/sync_harness.rs` (extend)

**Interfaces:**
- Consumes: `Error` type, Task 7 `is_transient`.
- Produces: behavior — every IMAP/SMTP op has a timeout (const `const NETWORK_OP_TIMEOUT: Duration = Duration::from_secs(60)` in `sync.rs`, applied at the adapter call sites); zero `unwrap`/`expect` reachable from network input; timeouts classified transient by `is_transient`.

- [ ] **Step 1: Write failing test** — `timeout_is_transient` (mock delayed response: `get_message` returns `Err` within ~timeout bound and `is_transient(err)` true). Plus a static check step in CI is unnecessary — the regression is the compile-time cleanup + this test.
- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p origami-core --test sync_harness timeout_is_transient`
Expected: FAIL

- [ ] **Step 3: Replace panics with `Result` propagation; wrap op awaits in `tokio::time::timeout`.**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src
git commit -m "fix: timeouts and panic hygiene on network paths"
```

### Task 12: Lock hygiene audit (incl. prefetch queue)

**Files:**
- Modify: `crates/origami-core/src/sync.rs` (`body_locks`, `prefetch_locks`), `crates/origami-core/src/prefetch_queue.rs` (if open-speed plan landed first)
- Test: `crates/origami-core/tests/prefetch_runtime.rs` or in-module tests (extend)

**Interfaces:**
- Consumes: existing lock maps.
- Produces: behavior — no mutex guard held across an `.await` on a network call (audit + fix); lock waits bounded (`tokio::time::timeout(30s, lock.lock())` returning `Err` instead of hanging); regression test `body_lock_wait_bounded` (hold a body lock in-test beyond 30s wait path with a tiny test-only bound — parameterize the bound for testability: `fn lock_with_timeout(map, key, bound: Duration)`).

- [ ] **Step 1: Write failing test** — `body_lock_wait_bounded` (hold lock, attempt second acquisition with `bound = 50ms`: `Err` returned, no deadlock).
- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p origami-core body_lock_wait_bounded`
Expected: FAIL

- [ ] **Step 3: Audit + fix guard-across-await sites; implement bounded waits.**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace --locked`
Expected: PASS

- [ ] **Step 5: Commit** (if authorized)

```bash
git add crates/origami-core/src crates/origami-core/tests
git commit -m "fix: bounded lock waits and no guards across network awaits"
```
