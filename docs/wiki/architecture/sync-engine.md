---
title: Sync engine
type: architecture
status: current
updated: 2026-09-27
sources:
  - docs/adr/0003-sync-algorithm.md
  - docs/PLAN.md
  - crates/origami-core/src/sync.rs
---

# Sync engine

Per-account actor loops in `crates/origami-core/src/sync.rs` keep the local SQLite store
converged with the server. The algorithm is CONDSTORE-first and envelope-first
([[adr-0003-sync-algorithm]]).

## Per-folder algorithm

1. `ENABLE QRESYNC` → `SELECT (CONDSTORE)`; compare stored **UIDVALIDITY**. A mismatch
   drops that folder's rows and triggers a full resync **of that folder only**.
2. **Delta pull**: new messages via `UID FETCH <last_uid+1>:*`; flag changes via
   `UID FETCH 1:* (FLAGS) (CHANGEDSINCE <modseq>)`. Upsert into SQLite, then emit UI events.
3. **Live updates**: IDLE on INBOX (fallback 60 s NOOP poll); on wake, run a delta pull.
4. **Envelope-first, body-on-demand**: headers sync eagerly; bodies fetch on open, with
   bounded background prefetch of the newest N messages for display cache. Raw RFC 822 goes
   to the content-addressed blob store; FTS5 is indexed after body fetch.
5. **Offline ops** (flags, moves, deletes, sends) apply to SQLite immediately and queue in
   `outbox`; replay in order on reconnect ([[offline-outbox]]).
6. **No CONDSTORE**: detected via `CAPABILITY`; fall back to `UID FETCH 1:* (FLAGS)` polls
   comparing a flags digest.

Per-folder `(UIDVALIDITY, HIGHESTMODSEQ, last_uid)` is stored in `sync_state`.

## Failure isolation and retry policy

- **Folder isolation**: `sync_account_inner` accumulates per-folder errors and keeps
  syncing the rest; one failing folder never kills the account sync.
- **Classification**: `is_transient` (`crates/origami-core/src/sync.rs`) treats IO and
  network-flavored backend errors (`timeout`, `connection`, `reset`, `BYE`, `watch error`)
  as transient; auth/config/data errors are permanent.
- **Backoff**: exponential from 2 s to a 300 s cap with ±20 % jitter
  (`backoff_delay`, clamped at the cap). Permanent errors skip straight to a 60 s slow
  mode — **the account loop never dies and never hot-retries**, so a fixed config
  self-heals. `ReconnectLimiter` caps reconnect churn: from the 10th consecutive failure
  the wait is a fixed 60 s, reset on success.
- **Network bounds**: every long-pole IMAP/SMTP operation (connect, list, fetch, search,
  select, flag/move/delete, append, SMTP send) runs under `with_network_timeout`
  (60 s); a timeout surfaces as a transient error so the retry policy applies. Per-message
  body locks use `acquire_bounded` (30 s) — a stuck fetch can no longer wedge later work
  forever; the recent-prefetch lock skips when busy instead of queueing.
- **Account health**: the app layer skips prefetch for accounts in the error map; sync
  events drive the UI "syncing" marker (the loop itself never clears it on exit, avoiding a
  restart race).

## Outbox replay

Replay runs oldest-first with the same classification. On failure the op is marked with an
incremented attempt count and a redacted error string (patterns plus the account's resolved
secrets). Permanent failures — and transient ones that reach the 5-attempt bound — move to
a terminal `failed_at` state that replay skips; the Outbox inspector surfaces them with a
per-row **Retry** that reopens the op (`outbox_reopen` resets attempts). Details:
[[offline-outbox]].

## Consequences

- Convergence is O(delta), not O(folder).
- All sync decisions live in one module, with a journal table for debugging.
- Trade-off: QRESYNC `VANISHED` handling adds complexity to the delete path.

## Test surface

`crates/origami-core/tests/sync_harness.rs` proves against Dovecot: initial sync populates
the store; the second sync is a cheap delta (0 added, 0 changed); `ensure_body` fetches,
blobs, and FTS-indexes a message; outbox flag ops replay to the server; and the QRESYNC/IDLE
watch stream observes an APPEND from a second session. `sync_plan.rs` and `sync_status.rs`
cover planning and status; `prefetch_runtime.rs` guards the runtime-less prefetch path and
the queued-display write path. Retry primitives (`is_transient`, `backoff_delay`,
`ReconnectLimiter`, `retry_wait_for`, `acquire_bounded`) have in-module unit tests. The
failure-isolation and poison-replay **end-to-end** runs against Docker are still owed —
see [[release-readiness]]. See also [[test-suite-md]].

## Related

- [[condstore-qresync]] · [[offline-outbox]] · [[store-and-search]] ·
  [[display-cache-and-prefetch]] · [[known-drift]]
