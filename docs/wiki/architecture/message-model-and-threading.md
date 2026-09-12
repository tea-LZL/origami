---
title: Message model and threading
type: architecture
status: current
updated: 2026-09-12
sources:
  - crates/origami-core/src/model.rs
  - crates/origami-core/src/message.rs
  - crates/origami-core/src/threading.rs
  - docs/PLAN.md
---

# Message model and threading

## Domain model

`crates/origami-core/src/model.rs` holds the provider-neutral domain types
(`Account`, `Mailbox`, `Envelope`, `Flag`, `Message`) and the **physical source identity**.
Every read, flag, label, move, delete, attachment fetch, and sync operation is addressed by a
physical `(mailbox_id, server_uid)` source — not by the logical message
([[logical-vs-physical-message]]).

## MIME and display data

`crates/origami-core/src/message.rs` parses MIME with `mail-parser` and produces normalized
display data. Two performance rules from the improvement plan are implemented here:

- fetch only the **display MIME sections** before large attachments;
- **cache normalized parsed MIME** so repeated opens do not re-parse.

Subject decoding uses `rfc2047-decoder`; the Dovecot harness seeds an RFC 2047 encoded
subject specifically to verify non-ASCII envelope listing ([[test-suite-md]]).

## Threading

`crates/origami-core/src/threading.rs` runs a **jwz-style** pass over `References` and
`In-Reply-To`. Envelopes are fetched with those headers and canonical thread roots are
persisted, enabling normalized-subject conversation grouping with expandable rows and
reading-pane navigation.

## Ordering

Chronological order uses IMAP `INTERNALDATE` / `received_at`, with the RFC `Date` header as
fallback — never the local clock alone.

## Related

- [[logical-vs-physical-message]] · [[store-and-search]] · [[sync-engine]] · [[origami-core]]
