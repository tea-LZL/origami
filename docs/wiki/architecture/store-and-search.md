---
title: Store and search
type: architecture
status: current
updated: 2026-09-12
sources:
  - docs/PLAN.md
  - crates/origami-core/src/store.rs
  - crates/origami-core/src/blob.rs
  - crates/origami-core/src/private_fs.rs
---

# Store and search

## One global database

`~/.local/share/origami/db.sqlite3`, SQLite in **WAL** mode. One database, one view layer —
deliberately not per-folder databases or bespoke formats (the Thunderbird lesson adopted in
[[plan-md]]).

Logical tables include: `accounts`, `folders`, `messages`, `threads`, `attachments`,
`tags`, `contacts`, `sync_state`, and `outbox`.

- `messages` carries an **app-owned UUIDv7 primary key**; `server_uid`, `folder_id`, and the
  `message_id` header are plain attributes ([[adr-0001-app-owned-keys]]).
- `sync_state` holds per-folder `(UIDVALIDITY, HIGHESTMODSEQ, last_uid)`.
- `outbox` holds pending offline mutations ([[offline-outbox]]).

Schema and migrations live in `crates/origami-core/src/store.rs`.

## Search

A single FTS5 index over subject / from / to / body-plain with the porter tokenizer. The
index is updated lazily **after the body is fetched** — envelope-first sync means subject and
participants are searchable before a body is downloaded.

## Bodies

Raw RFC 822 messages are stored as content-addressed blobs under `blobs/<aa>/<hash>`;
attachments are decoded on demand. This is deliberate *files*, not mbox/Mork
([[content-addressed-store]]).

## File permissions

`crates/origami-core/src/private_fs.rs` enforces `0700` roots and `0600`
config/database/new-blob files on open/write, hardening the previously observed `0755`
directories and `0644` files. An existing-data native smoke is still outstanding —
see [[release-readiness]].

## Related

- [[content-addressed-store]] · [[logical-vs-physical-message]] · [[sync-engine]]
