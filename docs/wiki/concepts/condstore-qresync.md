---
title: CONDSTORE / QRESYNC
type: concept
status: current
updated: 2026-09-12
sources:
  - docs/adr/0003-sync-algorithm.md
  - docs/TEST_SUITE.md
  - crates/origami-core/src/imap.rs
---

# CONDSTORE / QRESYNC

IMAP extensions that make delta sync cheap. Origami is **CONDSTORE-first**
([[adr-0003-sync-algorithm]]).

| Term | Meaning |
|---|---|
| CONDSTORE | Per-mailbox `HIGHESTMODSEQ`; lets a client ask "what changed since modseq X?" |
| QRESYNC | Quick resync: `VANISHED` reporting and expunge awareness alongside flag changes |
| UIDVALIDITY | Generation counter for a mailbox's UID space; a flip invalidates stored UIDs |

## How Origami uses it

1. `ENABLE QRESYNC` → `SELECT (CONDSTORE)`; compare UIDVALIDITY — mismatch means a full
   resync of that folder.
2. New messages: `UID FETCH <last_uid+1>:*`.
3. Flag changes: `UID FETCH 1:* (FLAGS) (CHANGEDSINCE <modseq>)`.
4. Persist `(UIDVALIDITY, HIGHESTMODSEQ, last_uid)` per folder in `sync_state`.

## Fallback

When `CAPABILITY` does not advertise CONDSTORE, the engine polls
`UID FETCH 1:* (FLAGS)` and compares a flags digest instead.

## Gotcha

`imap-codec 2.0.0-alpha.8` mis-decodes `CONDSTORE`/`QRESYNC` from the `CAPABILITY`
response, so `crates/origami-core/src/imap.rs` cross-references the raw response text and
patches the capability list in `connect_blocking` ([[backend-seam]], [[test-suite-md]]).

## Related

- [[sync-engine]] · [[adr-0003-sync-algorithm]] · [[offline-outbox]]
