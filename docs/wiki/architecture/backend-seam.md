---
title: Backend seam
type: architecture
status: current
updated: 2026-09-12
sources:
  - docs/adr/0002-backend-trait.md
  - crates/origami-core/src/backend.rs
  - crates/origami-core/src/imap.rs
  - crates/origami-core/src/smtp.rs
  - crates/origami-core/Cargo.toml
---

# Backend seam

`origami-core` defines two async traits over Origami domain types:

- `MailBackend` — read: connect, list mailboxes, list envelopes, fetch messages, flags,
  move/copy.
- `SmtpSender` — send: transmit raw RFC 822.

They are re-exported from the crate root; the domain types live in [[origami-core]]'s
`model` module. See `crates/origami-core/src/backend.rs`.

## The isolation rule

**Exactly one module, `origami_core::imap`, may name `io-imap`/`io-smtp` types.** The sync
engine, store, and UI see only the traits and domain types. ([[adr-0002-backend-trait]])

This matters because the protocol crates are pre-1.0:

- `io-imap` is git-pinned to a specific revision
  (`f57418aebc1491cdc0aed4135d5627f3349370db`) with `client`, `rustls-ring`, `scram`.
- `io-smtp` is a released `0.2` dependency.

Upgrades happen only as deliberate bump commits, with the Docker protocol harness green
([[test-suite-md]]).

## Why this shape

`io-imap`/`io-smtp` give correct, extension-rich protocol clients (IDLE,
CONDSTORE/QRESYNC, UIDPLUS, MOVE, SORT/THREAD, OAUTHBEARER/XOAUTH2, SCRAM), but their APIs
will move between revisions. Isolating them means upstream churn touches one module, and
future JMAP/Gmail-API/Graph backends are new trait implementations rather than sync/UI
surgery. The seam is modeled on Himalaya's `src/shared/client.rs`; Himalaya itself is
binary-only and is not a dependency.

## Known upstream defects

Documented in [[test-suite-md]]:

- `imap-codec 2.0.0-alpha.8` mis-decodes `CONDSTORE`/`QRESYNC` from `CAPABILITY`; the
  adapter cross-references the raw response text and patches the capability list
  (`crates/origami-core/src/imap.rs`, `connect_blocking`).
- `io-smtp 0.2.0` rejects GreenMail's non-RFC `220 /<ip> ...` greeting; the send E2E test
  degrades to a diagnostic skip rather than a hard failure.

## Related

- [[adr-0002-backend-trait]] · [[protocol-dependencies]] · [[runtime-and-layers]]
