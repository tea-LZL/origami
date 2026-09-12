---
title: Content-addressed store
type: concept
status: current
updated: 2026-09-12
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

## Related

- [[store-and-search]] · [[sync-engine]] · [[message-model-and-threading]]
