---
title: origami-core
type: entity
status: current
updated: 2026-09-27
sources:
  - crates/origami-core/src/lib.rs
  - crates/origami-core/Cargo.toml
---

# origami-core

The UI-agnostic Rust library crate: backend abstraction, sync engine, local store. Both
`origami-app` and `origami-cli` build on it; it declares **no Tauri dependency**
(`crates/origami-core/src/lib.rs`).

## Modules

| Module | Responsibility |
|---|---|
| `backend.rs` | `MailBackend` (read) and `SmtpSender` (send) traits |
| `imap.rs` | IMAP adapter; the only module allowed to name `io-imap` |
| `smtp.rs` | SMTP adapter |
| `store.rs` | SQLite schema, migrations, FTS, projection, cache, outbox |
| `blob.rs` | Content-addressed blob store with a 64-hex hash gate |
| `sync.rs` | Per-account sync loops, recent-message prefetch, retry policy (`is_transient`, jittered backoff, `ReconnectLimiter`, `with_network_timeout`, `acquire_bounded`) |
| `prefetch_queue.rs` | Priority display-prefetch queue (dedupe, cancel/generation, failure kill switch) |
| `message.rs` | MIME parsing and normalized display data (depth/header/decode caps) |
| `threading.rs` | jwz-style threading |
| `model.rs` | Provider-neutral domain models and physical source identity |
| `compose.rs` | Message building (`mail-builder`) |
| `config.rs` | Account TOML config and secret indirection |
| `oauth.rs` | OAuth2 PKCE and token management |
| `provider_hints.rs` | Provider detection hints for onboarding |
| `redact.rs` | Secret redaction for errors and persisted diagnostics |
| `private_fs.rs` | `0700`/`0600` permission enforcement, symlink-safe tree tightening (private module) |
| `error.rs` | Error and Result types |

The crate re-exports `MailBackend`, `SmtpSender`, `Error`, `Result`, and `version()`.

## Key dependencies

`rusqlite` (bundled SQLite), `mail-parser`, `mail-builder`, `rfc2047-decoder`, `reqwest`
(rustls), `keyring` v3, `uuid` v7, `sha2`, `secrecy`, `toml`, `tokio`, plus the pinned
protocol crates ([[protocol-dependencies]]).

## Tests

`imap_harness.rs`, `sync_harness.rs`, `send_e2e.rs`, `store.rs`, `sync_plan.rs`,
`sync_status.rs`, `prefetch_runtime.rs`, `hostile_mime.rs` — the live ones are env-gated
([[test-suite-md]]). In-module unit tests cover the prefetch queue, retry primitives, store
invariants, and redaction.

## Related

- [[backend-seam]] · [[sync-engine]] · [[store-and-search]] · [[runtime-and-layers]]
