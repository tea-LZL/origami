---
title: ADR-0002 — Backend trait isolates protocol crates
type: decision
status: current
updated: 2026-09-12
sources:
  - docs/adr/0002-backend-trait.md
---

# ADR-0002 — Backend trait isolates protocol crates

**Status:** Accepted (2026-07-19) · **Source:** `docs/adr/0002-backend-trait.md`

## Decision

`origami-core` defines `MailBackend` (read) and `SmtpSender` (send) async traits over
Origami domain types. Exactly one module, `origami_core::imap`, may name
`io-imap`/`io-smtp` types; the sync engine, store, UI, and CLI see only the traits and
domain types.

Dependencies are pinned to git revisions, and upgrades happen only via deliberate bump
commits with the Dovecot test harness green.

## Why

The pimalaya `io-imap`/`io-smtp` crates give correct, extension-rich protocol clients
(IDLE, CONDSTORE/QRESYNC, UIDPLUS, MOVE, SORT/THREAD, OAUTHBEARER/XOAUTH2, SCRAM), but they
are pre-1.0 and their APIs will break between revisions. Himalaya itself is binary-only and
cannot be depended on; its `src/shared/client.rs` is the model for this seam.

## Consequences

- Upstream churn touches one module ([[backend-seam]]).
- JMAP / Gmail-API / Graph backends can be added as new trait implementations without
  sync or UI changes.
- The sans-IO internals stay an implementation detail: today blocking std clients run on
  `spawn_blocking`; a fully async adapter can replace them behind the same trait
  ([[runtime-and-layers]]).

## Related

- [[backend-seam]] · [[protocol-dependencies]] · [[adr-0003-sync-algorithm]]
