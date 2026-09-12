---
title: Runtime and layers
type: architecture
status: current
updated: 2026-09-12
sources:
  - docs/PLAN.md
  - crates/origami-core/src/lib.rs
  - Cargo.toml
---

# Runtime and layers

## Process model

Origami is a **single process**. A tokio multi-thread runtime owns sync work; the webview
runs on the main thread. There is no daemon and no startup service: sync, cache warming,
outbox replay, notifications, and UI commands all run inside the running app.

The hard responsiveness rule: **the UI never blocks on IMAP.** The UI reads only from
SQLite; sync pushes deltas to it as Tauri events. This is the core guarantee behind the
"offline-first" claim.

## Layers

```text
Svelte 5 UI (webview)
  -> typed Tauri commands (invoke) / events (sync deltas, new mail)
origami-app  (Tauri shell)
  -> AppState: config + Store + SyncEngine + cancellation ownership
  -> keyring, tray, notifications, OAuth redirect listener
origami-core (pure Rust, no Tauri deps)
  -> Store (SQLite/WAL + FTS5 + content-addressed blobs)
  -> SyncEngine (per-account actors)
  -> MailBackend / SmtpSender
  -> IMAP / SMTP servers
```

| Layer | Crate / dir | Responsibility |
|---|---|---|
| Frontend | `ui/` | Presentation, UI state, optimistic actions |
| Shell | `crates/origami-app` | Commands, events, window, tray, notifications, secrets |
| Core | `crates/origami-core` | Domain model, store, sync, protocol adapters |
| Debug | `crates/origami-cli` | Operator smoke commands over core |

`origami-core` declares no Tauri dependency; both the shell and the CLI build on it
(`crates/origami-core/src/lib.rs`).

## Concurrency

One actor per account (`AccountActor`) owns that account's IMAP connection pool — one
session reserved for IDLE on INBOX plus a small pool for fetches — and receives work on a
`tokio::mpsc` command mailbox. The blocking `io-*` std clients are pumped on
`tokio::task::spawn_blocking` workers. Because `io-imap` is sans-IO at its core, a fully
async adapter can replace this later **behind the same trait** ([[adr-0002-backend-trait]]).

## What this implies

- Provider logic cannot leak upward: the UI receives normalized DTOs only ([[backend-seam]]).
- Cancellation is owned by `AppState`, not by individual commands.
- Adding a new backend (JMAP, Gmail API, Graph) does not change sync or UI.

## Related

- [[sync-engine]] · [[store-and-search]] · [[ui-state-and-rendering]] · [[overview]]
