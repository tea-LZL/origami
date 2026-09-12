---
title: ADR-0001 — Origami owns all primary keys
type: decision
status: current
updated: 2026-09-12
sources:
  - docs/adr/0001-message-key-ownership.md
---

# ADR-0001 — Origami owns all primary keys

**Status:** Accepted (2026-07-19) · **Source:** `docs/adr/0001-message-key-ownership.md`

## Decision

Every local entity (account, mailbox, message, thread, attachment) gets an **app-owned
UUIDv7 primary key**. Server identifiers — IMAP UID, UIDVALIDITY, `Message-ID`,
RFC 8474 OBJECTID — are stored as plain attributes, with uniqueness enforced only where the
protocol guarantees it (`(mailbox_id, server_uid)` is unique per `sync_state` generation).

## Why

IMAP UIDs are only unique within `(folder, UIDVALIDITY)`, can be renumbered after a
UIDVALIDITY flip, and the same message can exist in multiple folders (Gmail labels,
RFC 8474 OBJECTID). Thunderbird's Panorama ADR-0002 documents how using server UIDs as local
keys corrupted their entire data layer — the failure mode this decision exists to avoid.

## Consequences

- Sync maps server → local keys on every pull via `sync_state` / `messages(server_uid)`
  lookups ([[sync-engine]]).
- A UIDVALIDITY flip is a folder-local resync, not a database-wide crisis.
- Cross-folder deduplication by `Message-ID`/OBJECTID is possible later without key surgery,
  and is the foundation of [[logical-vs-physical-message]].

## Related

- [[logical-vs-physical-message]] · [[store-and-search]] · [[adr-0003-sync-algorithm]]
