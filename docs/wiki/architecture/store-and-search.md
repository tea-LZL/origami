---
title: Store and search
type: architecture
status: current
updated: 2026-09-27
sources:
  - docs/PLAN.md
  - crates/origami-core/src/store.rs
  - crates/origami-core/src/blob.rs
  - crates/origami-core/src/private_fs.rs
  - crates/origami-core/tests/store.rs
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

## Envelope listing and unread-only

`list_envelopes_in_folders` reads all matching physical rows, then
`deduplicate_envelopes`, then optional unread filtering, then skip/take. Pagination
cannot run first: provider labels are separate IMAP rows, and paging before dedupe would
let one logical message occupy several page slots.

When `unread_only` is true, the store retains logical envelopes whose merged flags lack
`Seen`. Unread is defined by `merge_envelope_sources`: Seen is on the logical envelope
only if **every** retained physical copy is Seen. Filtering on SQL `flags_json` before
dedupe would drop label copies from `sources` ([[logical-vs-physical-message]],
[[unread-only-list-filter]]). `list_unified_inbox` uses the same flag and the same
order. Commands default `unread_only` to false.

## Search

A single FTS5 index over subject / from / to / body-plain with the porter tokenizer. The
index is updated lazily **after the body is fetched** — envelope-first sync means subject and
participants are searchable before a body is downloaded. The unread-only **search** path
does not use the store argument: the UI appends an `is:unread` token
([[ui-state-and-rendering]]).

## Bodies

Raw RFC 822 messages are stored as content-addressed blobs under `blobs/<aa>/<hash>`;
attachments are decoded on demand. This is deliberate *files*, not mbox/Mork
([[content-addressed-store]]). Blob lookups are gated by a content-address check: only an
exact 64-character lowercase-hex SHA-256 digest may name a blob, so a hostile "hash" cannot
traverse outside the blob root.

## Parse cache (`message_cache`)

Normalized parsed data (display MIME or full-body) lives in `message_cache` keyed by
message id, alongside `cached_at` — a **pure cache** separate from `messages` and blobs.
`evict_display_cache(max_rows, max_age_days)` bounds it (2,000 rows / 30 days, run after
warm batches), ordering by `COALESCE(received_at, cached_at)` and keeping the newest rows;
deleting cache rows never touches blobs, FTS, or envelopes. The in-memory LRU in front of
it is described in [[display-cache-and-prefetch]].

## Write integrity

- Multi-row writes are transactional: `update_flags_batch`, `update_keywords_batch`,
  `delete_messages_by_uids`, and `update_flags_with_outbox` (flags/keywords + outbox row in
  one transaction) commit atomically — partial batches are never visible. Atomicity is
  pinned by sqlite `RAISE(ABORT)` trigger tests in `crates/origami-core/src/store.rs`.
- Tray quit runs `stop_all_sync` then `shutdown_flush` (`PRAGMA wal_checkpoint(TRUNCATE)`),
  and the composer flushes its draft on `beforeunload`, so a hard exit leaves a
  checkpointed database.
- `foreign_keys` is ON at every connection open; `assert_store_invariants` checks for
  orphan message and FTS rows (test/debug helper).

## File permissions

`crates/origami-core/src/private_fs.rs` enforces `0700` roots and `0600`
config/database/new-blob files on open/write. The existing-data smoke landed in PR #1
(`existing_data_perms_tightened` in `crates/origami-core/tests/store.rs`): a fixture with
legacy `0755`/`0644` directories, database sidecars (`-wal`/`-shm`), blob shards, and config
files reopens through the real app paths and asserts tightened modes with contents
byte-unchanged. `secure_existing_tree` skips symlinks so chmod never escapes the tree.

## Related

- [[content-addressed-store]] · [[logical-vs-physical-message]] · [[sync-engine]] ·
  [[display-cache-and-prefetch]] · [[unread-only-list-filter]]
