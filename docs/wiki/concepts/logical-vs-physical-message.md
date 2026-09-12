---
title: Logical vs physical message
type: concept
status: current
updated: 2026-09-12
sources:
  - MEMORY.md
  - docs/adr/0001-message-key-ownership.md
  - crates/origami-core/src/model.rs
---

# Logical vs physical message

Origami gives a message **two identities** at once.

| Identity | Key | Used for |
|---|---|---|
| Logical | Origami-owned UUIDv7 | Deduplicated presentation, conversations |
| Physical | `(mailbox_id, server_uid)` | Reads, flags, labels, moves, deletes, attachments, sync |

The same logical message can have several physical sources — the canonical case is a Gmail
message carrying multiple labels, which appears in several mailboxes with distinct UIDs, or
one message delivered to two folders.

## Rules

- **Never treat an IMAP UID as globally unique.** It is unique only within
  `(folder, UIDVALIDITY)` ([[adr-0001-app-owned-keys]]).
- **Never discard a retained physical source** when deduplicating logical messages.
- Address every mutation by physical source, even when the UI shows one logical row.

## Where it shows up

- `crates/origami-core/src/model.rs` defines the physical source identity.
- Cross-label deduplication is implemented in the store/sync projection
  ([[store-and-search]], [[sync-engine]]).
- The logical-Sent projection in [[hermes-sent-tray-background]] groups two physical folders
  under one sidebar entry — the same concept applied to a folder.

## Related

- [[adr-0001-app-owned-keys]] · [[message-model-and-threading]] · [[overview]]
