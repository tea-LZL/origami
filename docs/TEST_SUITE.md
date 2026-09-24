# Docker Test Suite

A local integration suite providing a real IMAP server (Dovecot) and SMTP+IMAP receiver (GreenMail) for running live protocol tests against Origami's core without needing a real email provider.

## Architecture

```
tests/harness/
├── docker-compose.yml          # 2 services
├── dovecot/
│   ├── dovecot.conf             # minimal config
│   ├── prepare-mail.sh          # restores ":" in Maildir names inside the container
│   └── mail/origami/cur/        # seeded messages; "__c__" stands for ":"
│       ├── 1752832801.0001.harness__c__2,S
│       ├── 1752841801.0002.harness__c__2,S
│       └── 1752912001.0003.harness__c__2,
└── README.md
```

## Two Services

### Dovecot (`origami-test-dovecot`)

- IMAP on `127.0.0.1:10143`
- Plaintext only (no TLS), static single user `origami` / password `origami`
- Maildir backing store seeded with 3 RFC 822 messages
- Committed cur/ names use `__c__` instead of `:`. The container rewrites them to Maildir `:2,flags` before Dovecot starts. `...,S` is `\Seen`. The third message is `:2,` (no flags).
- Full CONDSTORE, QRESYNC, and IDLE support
- **Purpose**: tests sync, flag, and fetch against a real RFC-compliant IMAP server

### GreenMail (`origami-test-greenmail`)

- SMTP on `127.0.0.1:30025`, IMAP on `127.0.0.1:30143`
- Users are auto-created on first SMTP delivery; the password is the local-part of the recipient address
- **Purpose**: IMAP receive tests against a second provider (verifies no Dovecot-specific coupling), plus SMTP send tests

## How Tests Use It

All tests under `crates/origami-core/tests/` are env-var gated (`ORIGAMI_TEST_IMAP=1`, `ORIGAMI_TEST_SMTP=1`). Without the variable they pass trivially (no-op), so CI without Docker stays green.

| Test file | Tests | What it proves |
|---|---|---|
| `imap_harness.rs` | 4 | Connect, list mailboxes, envelope listing, RFC 2047 subject decode, `BODY.PEEK` does not set `\Seen`, flag store roundtrip — all against Dovecot |
| `sync_harness.rs` | 5 | Initial sync populates the local SQLite store; second sync is a cheap delta (0 added, 0 changed); `ensure_body` fetches the raw message, stores it in the content-addressed blob store, and indexes it in FTS5; outbox flag ops replay to the server; the QRESYNC/IDLE watch stream observes an APPEND from a second session — all against Dovecot |
| `send_e2e.rs` | 2 | Build a multipart message, send via SMTP (gracefully tolerates an io-smtp greeting parser bug against GreenMail's non-RFC `220 /ip ...` greeting); receive via GreenMail IMAP and parse the body — against GreenMail |

## Invocation

```sh
docker compose -f tests/harness/docker-compose.yml up -d

# IMAP-only tests
ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test imap_harness
ORIGAMI_TEST_IMAP=1 cargo test -p origami-core --test sync_harness

# IMAP + SMTP (requires GreenMail)
ORIGAMI_TEST_IMAP=1 ORIGAMI_TEST_SMTP=1 cargo test -p origami-core --test send_e2e

# All live tests (separate --test flags, Cargo does not accept pipe patterns)
ORIGAMI_TEST_IMAP=1 ORIGAMI_TEST_SMTP=1 \
  cargo test -p origami-core \
    --test imap_harness \
    --test sync_harness \
    --test send_e2e

docker compose -f tests/harness/docker-compose.yml down
```

## Seeded Messages

| File | Subject | Flags | Notes |
|---|---|---|---|
| `1752832801.0001.harness__c__2,S` | Welcome to Origami | Seen | Plain text, `From: Alice Anders` with display name. Runtime name ends in `:2,S`. |
| `1752841801.0002.harness__c__2,S` | Launch tomorrow? | Seen | Subject is RFC 2047 base64-encoded (`=?UTF-8?B?TGF1bmNoIHRvbW9ycm93Pw==?=`). Runtime name ends in `:2,S`. |
| `1752912001.0003.harness__c__2,` | Weekly digest | _none_ | Multipart/alternative. Runtime name ends in `:2,`. |

The RFC 2047 encoded subject is specifically there to verify that `decode_mime_words` decodes non-ASCII subjects correctly during envelope listing.

## Design Notes

- Tests are isolated by using separate temp directories for the local SQLite store per test, so they do not interfere with each other's database state
- The IDLE watch test creates a dedicated `OrigamiWatch` mailbox to avoid disturbing the seeded INBOX counts that other tests depend on
- A known **imap-codec 2.0.0-alpha.8 parser bug** mis-decodes `CONDSTORE` and `QRESYNC` from the `CAPABILITY` response. We work around this by cross-referencing the raw `CAPABILITY` response text and patching the capability list (`crates/origami-core/src/imap.rs` — see the `connect_blocking` workaround)
- A known **io-smtp 0.2.0 greeting parser bug** rejects GreenMail's `220 /172.18.0.3 ...` greeting (the `/` before the IP is non-RFC). The send test handles this gracefully with a diagnostic skip rather than a hard failure
