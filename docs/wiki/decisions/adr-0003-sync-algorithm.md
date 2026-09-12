---
title: ADR-0003 — Sync algorithm (CONDSTORE-first, envelope-first)
type: decision
status: current
updated: 2026-09-12
sources:
  - docs/adr/0003-sync-algorithm.md
---

# ADR-0003 — Sync algorithm (CONDSTORE-first, envelope-first)

**Status:** Accepted (2026-07-19) · **Source:** `docs/adr/0003-sync-algorithm.md`

## Decision

1. Per folder: `ENABLE QRESYNC` → `SELECT (CONDSTORE)`; compare stored UIDVALIDITY — a
   mismatch drops the folder's rows and forces a full resync.
2. Delta pull: new messages via `UID FETCH <last_uid+1>:*`; flag changes via
   `UID FETCH 1:* (FLAGS) (CHANGEDSINCE <modseq>)`; upsert into SQLite, then emit UI events.
   Store per-folder `(UIDVALIDITY, HIGHESTMODSEQ, last_uid)` in `sync_state`.
3. Live updates: IDLE on INBOX (fallback 60 s NOOP poll); wake → delta pull.
4. Envelope-first: headers sync eagerly; bodies fetch on open; background prefetch of the
   newest N; raw RFC 822 in the content-addressed blob store; FTS5 indexed after body fetch.
5. Offline ops (flags, moves, deletes, sends) apply to SQLite immediately and queue in
   `outbox`; replay in order on reconnect.
6. Servers without CONDSTORE (detected via `CAPABILITY`) fall back to
   `UID FETCH 1:* (FLAGS)` polls comparing a flags digest.

## Why

The UI must never block on IMAP; the local DB must converge with the server efficiently; and
offline edits must not lose data. `io-imap` ships the required machinery
(ENABLE QRESYNC, SELECT (CONDSTORE), FETCH CHANGEDSINCE, IDLE) through its coroutine
`watch` module.

## Consequences

- Convergence is O(delta), not O(folder).
- All sync decisions live in one module (`origami_core::sync`) with a journal table for
  debugging.
- Trade-off: QRESYNC `VANISHED` handling adds complexity to the delete path.

## Related

- [[sync-engine]] · [[condstore-qresync]] · [[offline-outbox]] · [[adr-0001-app-owned-keys]]
