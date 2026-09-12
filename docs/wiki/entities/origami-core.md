---
title: origami-core
type: entity
status: current
updated: 2026-09-12
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
| `blob.rs` | Content-addressed blob store |
| `sync.rs` | Per-account sync loops and recent-message prefetch |
| `message.rs` | MIME parsing and normalized display data |
| `threading.rs` | jwz-style threading |
| `model.rs` | Provider-neutral domain models and physical source identity |
| `compose.rs` | Message building (`mail-builder`) |
| `config.rs` | Account TOML config and secret indirection |
| `oauth.rs` | OAuth2 PKCE and token management |
| `provider_hints.rs` | Provider detection hints for onboarding |
| `private_fs.rs` | `0700`/`0600` permission enforcement (private module) |
| `error.rs` | Error and Result types |

The crate re-exports `MailBackend`, `SmtpSender`, `Error`, `Result`, and `version()`.

## Key dependencies

`rusqlite` (bundled SQLite), `mail-parser`, `mail-builder`, `rfc2047-decoder`, `reqwest`
(rustls), `keyring` v3, `uuid` v7, `sha2`, `secrecy`, `toml`, `tokio`, plus the pinned
protocol crates ([[protocol-dependencies]]).

## Tests

`imap_harness.rs`, `sync_harness.rs`, `send_e2e.rs`, `store.rs`, `sync_plan.rs`,
`sync_status.rs` — the live ones are env-gated ([[test-suite-md]]).

## Related

- [[backend-seam]] · [[sync-engine]] · [[store-and-search]] · [[runtime-and-layers]]
