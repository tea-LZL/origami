---
title: Content-addressed store
type: concept
status: current
updated: 2026-09-27
sources:
  - docs/PLAN.md
  - crates/origami-core/src/blob.rs
---

# Content-addressed store

Raw RFC 822 message bodies are stored as **content-addressed blobs** under
`blobs/<aa>/<hash>` — two hex characters of fan-out, then the content hash. Attachments are
decoded on demand rather than eagerly.

## Why files, not a database

Deliberate choice from [[plan-md]]: bodies are kept as files, not mbox or Mork. The database
stays a metadata/search index; large immutable payloads live outside it and are deduplicated
by construction (identical content hashes to one blob).

## Interaction with sync

- Envelope-first sync means a message row exists before its body blob does.
- `ensure_body` fetches the raw message, writes the blob, and only then updates FTS5 —
  proven end-to-end in the Dovecot harness ([[test-suite-md]]).
- Cache warming is **display-only**: it never prefetches attachment bytes or remote
  resources.

## Access and decode gates

- Blob reads and existence checks validate the hash as an exact 64-character lowercase-hex
  SHA-256 digest (`validated_hash` in `blob.rs`) before any path join — a hostile "hash"
  such as `../../etc/passwd` is rejected, so traversal outside the blob root is impossible.
- Attachment decoding refuses inputs/outputs over `MAX_ATTACH_DECODE_BYTES` (64 MiB) and
  MIME parsing is bounded by depth/header caps ([[store-and-search]]); see the hostile-MIME
  corpus in `crates/origami-core/tests/hostile_mime.rs`.

## Related

- [[store-and-search]] · [[sync-engine]] · [[message-model-and-threading]] ·
  [[hardening-caching-palette-spec]]
