---
title: "Source: docs/TEST_SUITE.md"
type: source
status: current
updated: 2026-09-12
sources: [docs/TEST_SUITE.md]
---

# Source — `docs/TEST_SUITE.md`

**Ingested:** 2026-09-12 · The Docker live-protocol harness documentation (85 lines).

## What it is

Two services under `tests/harness/`:

- **Dovecot** (`origami-test-dovecot`) — IMAP on `127.0.0.1:10143`, plaintext, static user
  `origami`/`origami`, maildir seeded with 3 RFC 822 messages, full CONDSTORE/QRESYNC/IDLE.
- **GreenMail** (`origami-test-greenmail`) — SMTP on `127.0.0.1:30025`, IMAP on `30143`;
  users auto-created on first SMTP delivery; used for a second provider and SMTP send.

Tests are env-gated (`ORIGAMI_TEST_IMAP=1`, `ORIGAMI_TEST_SMTP=1`) so CI without Docker stays
green.

## Proven coverage

| Test file | Proves |
|---|---|
| `imap_harness.rs` | Connect, list mailboxes, envelope listing, RFC 2047 subject decode, `BODY.PEEK` does not set `\Seen`, flag roundtrip (Dovecot) |
| `sync_harness.rs` | Initial sync populates store; second sync is a cheap delta (0/0); `ensure_body` blobs + FTS-indexes; outbox flag replay; QRESYNC/IDLE observes an APPEND from a second session (Dovecot) |
| `send_e2e.rs` | Builds a multipart message, sends via SMTP tolerating an `io-smtp` greeting bug, receives via GreenMail IMAP and parses the body |

## Known defects recorded here

- `imap-codec 2.0.0-alpha.8` mis-decodes `CONDSTORE`/`QRESYNC` from `CAPABILITY`; worked
  around in `crates/origami-core/src/imap.rs`.
- `io-smtp 0.2.0` rejects GreenMail's non-RFC `220 /<ip> ...` greeting; the test degrades to
  a diagnostic skip instead of failing.

## Fed into

[[backend-seam]] · [[condstore-qresync]] · [[sync-engine]] · [[content-addressed-store]] ·
[[message-model-and-threading]] · [[release-readiness]] · [[build-and-verification]]
