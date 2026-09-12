---
title: Offline outbox
type: concept
status: current
updated: 2026-09-12
sources:
  - docs/adr/0003-sync-algorithm.md
  - docs/IMPROVEMENT_PLAN.md
  - crates/origami-core/src/store.rs
---

# Offline outbox

Offline operations are **applied to SQLite immediately** and queued in the `outbox` table;
on reconnect they replay **in order**. The UI therefore stays responsive and optimistic
while the server state converges later ([[adr-0003-sync-algorithm]]).

## What queues

- flag changes (read, unread, star, unstar), including batch operations;
- archive, move, trash, permanent delete;
- junk and label/keyword commands;
- SMTP sends (a durable send queue) and independent Sent-copy retries.

## Semantics

- Irreversible mail actions go through guarded commands with offline replay and a six-second
  optimistic **undo** window for destructive operations.
- Failed flag changes are queued rather than dropped.
- Outbox contents are inspectable through a sanitized **Outbox inspector** with retry
  controls; pending operations are surfaced in the status area.

## Related

- [[sync-engine]] · [[store-and-search]] · [[release-readiness]] · [[improvement-plan-md]]
