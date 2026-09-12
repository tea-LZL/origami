---
title: Protocol dependencies
type: entity
status: current
updated: 2026-09-12
sources:
  - crates/origami-core/Cargo.toml
  - docs/adr/0002-backend-trait.md
  - docs/PLAN.md
---

# Protocol dependencies

The pimalaya protocol stack, isolated behind Origami traits ([[adr-0002-backend-trait]]).

| Crate | Pin | Notes |
|---|---|---|
| `io-imap` | git rev `f57418aebc1491cdc0aed4135d5627f3349370db` | `client`, `rustls-ring`, `scram`; IDLE, CONDSTORE/QRESYNC, UIDPLUS, MOVE, SORT/THREAD, OAUTHBEARER/XOAUTH2, SCRAM |
| `io-smtp` | `0.2` | `client`, `rustls-ring`, `scram` |
| `pimalaya-stream` | `0.1` | `std`, `rustls-ring`; TLS plumbing |

Supporting crates: `mail-parser` (0.11) and `mail-builder` (0.4) for MIME,
`rfc2047-decoder` for headers, `imap-codec 2.0-alpha` under `io-imap`.

## Risk and mitigation

Top project risk #1 is **pimalaya alpha churn**. Mitigations: git-pinned revisions, the
backend-trait isolation, and `cargo update` only via deliberate bump commits with the
Dovecot harness green ([[test-suite-md]], [[plan-md]]).

Two concrete upstream defects are already worked around or documented: the
`imap-codec` CAPABILITY mis-decode and the `io-smtp` GreenMail greeting rejection
([[backend-seam]]).

## Why not Himalaya

Himalaya v2 is binary-only and cannot be depended on; its `io-*` crates are MIT/Apache and
are used directly, with its `src/shared/client.rs` as the model for the seam.

## Related

- [[backend-seam]] · [[adr-0002-backend-trait]] · [[known-drift]]
