---
title: Offline outbox
type: concept
status: current
updated: 2026-09-27
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

## Terminal failure (poison) handling

Replay failures classify through `is_transient` ([[sync-engine]]):

- **Permanent** failures (auth/config/data) move the op to a terminal state immediately
  (`failed_at` set, redacted `last_error` stored).
- **Transient** failures increment `attempts` and retry until the 5-attempt bound, then
  also go terminal — no infinite retry.
- Terminal rows are skipped by replay but remain listed; the inspector renders a **Failed**
  chip and a per-row **Retry** that reopens the op (`outbox_reopen`: clears `failed_at`,
  resets attempts and error). Pending counts exclude terminal rows, while the Outbox entry
  points sum pending + failed so a fully poisoned outbox stays reachable.
- Schema: `outbox.failed_at` (migration `user_version` 9). Persisted errors are redacted
  (pattern-based plus the account's resolved secrets) before they reach the store.

## Related

- [[sync-engine]] · [[store-and-search]] · [[release-readiness]] · [[improvement-plan-md]] ·
  [[hardening-caching-palette-spec]]
