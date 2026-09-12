---
title: origami-app
type: entity
status: current
updated: 2026-09-12
sources:
  - crates/origami-app/src/lib.rs
  - crates/origami-app/src/state.rs
  - crates/origami-app/src/commands.rs
  - crates/origami-app/tauri.conf.json
---

# origami-app

The Tauri 2 desktop shell. It owns the window, tray, notifications, secret access, and the
command/event boundary the Svelte UI talks to.

## Files

| File | Responsibility |
|---|---|
| `src/lib.rs` | Tauri lifecycle, tray, sync events, notifications |
| `src/state.rs` | `AppState`: store, config, workers, OAuth refresh, cancellation ownership |
| `src/commands.rs` | Tauri command / DTO boundary |
| `src/oauth_flow.rs` | Local-host redirect listener for the OAuth code |
| `src/notifications.rs` | Desktop notification plumbing |
| `src/main.rs` | Entry point |

Also present: `tauri.conf.json`, `capabilities/`, `icons/`, `gen/`, `build.rs`.

## Boundary contract

- Every UI request is a typed command returning normalized DTOs.
- Sync progress and new mail reach the UI as **events**, never as blocking calls.
- Cancellation is owned by `AppState` ([[runtime-and-layers]]).
- **Commands that touch Tokio must be `async`.** Tauri dispatches synchronous commands on
  the GTK main thread, where no Tokio runtime context exists; a `tokio::spawn` there
  panics, and because that panic crosses an FFI callback boundary it aborts the process.
  Regression guard: `crates/origami-core/tests/prefetch_runtime.rs`.

## Related

- [[accounts-and-secrets]] · [[ui-state-and-rendering]] · [[origami-core]]
