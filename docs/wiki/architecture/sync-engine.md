---
title: Sync engine
type: architecture
status: current
updated: 2026-09-12
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

## Consequences

- Convergence is O(delta), not O(folder).
- All sync decisions live in one module, with a journal table for debugging.
- Trade-off: QRESYNC `VANISHED` handling adds complexity to the delete path.

## Test surface

`crates/origami-core/tests/sync_harness.rs` proves against Dovecot: initial sync populates
the store; the second sync is a cheap delta (0 added, 0 changed); `ensure_body` fetches,
blobs, and FTS-indexes a message; outbox flag ops replay to the server; and the QRESYNC/IDLE
watch stream observes an APPEND from a second session. `sync_plan.rs` and `sync_status.rs`
cover planning and status. See [[test-suite-md]].

## Related

- [[condstore-qresync]] · [[offline-outbox]] · [[store-and-search]] · [[known-drift]]
