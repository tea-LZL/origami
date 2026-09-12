---
title: "Source: .hermes sent/tray plan"
type: source
status: current
updated: 2026-09-12
sources: [".hermes/plans/2026-08-09_140833-sent-tray-background.md"]
---

# Source — unified Sent mail and tray background lifecycle plan

**Ingested:** 2026-09-12 · Execution plan, 2026-08-09.

## What it is

An implementation plan (not a spec) for two coupled changes: a **pure logical-Sent mailbox
projection** that groups the physical Sent folders under one sidebar entry, and a **Tauri tray
background lifecycle** with explicit restore/quit.

## Task structure

1. Add a pure logical-Sent mailbox projection.
2. Make the logical Sent entry load messages from both physical folders.
3. Keep counts and live refresh correct for grouped folders.
4. Add the Tauri tray icon and explicit restore/quit actions.
5. Add focused native lifecycle coverage and run the full verification set.

Plus acceptance criteria and a risks/follow-up boundary. The "pure projection" framing is the
[[logical-vs-physical-message]] model applied at folder level.

## Verbatim excerpt — current context

```text
## Current context and confirmed facts

- `crates/origami-core/src/imap.rs:1294` maps both `Sent` and `[Gmail]/Sent Mail` to `MailboxRole::Sent`.
- `crates/origami-app/src/commands.rs:82-111` returns every physical folder from `list_folders`, and `ui/src/lib/Sidebar.svelte:213-231` renders every returned folder. `roleLabel()` changes both names to the visible label `Sent`.
- The live local database currently contains, for `lodashtea@gmail.com`:
  - `Sent`, role `sent`, 4 messages
  - `[Gmail]/Sent Mail`, role `sent`, 68 messages
- `crates/origami-app/src/state.rs:61-137` already starts long-running per-account sync loops. `crates/origami-app/src/lib.rs:50-114` already converts sync events into notifications and frontend events.
- There is currently no tray feature or close-request handler in `crates/origami-app/src/lib.rs`; closing the only window therefore ends the Tauri process and drops the sync tasks.
- `crates/origami-app/icons/icon.png` already exists and can be reused for the tray icon.
- Tauri's Linux tray documentation says tray click events are not emitted on Linux, although the icon and context menu work. The restore path must therefore be a tray menu item, not click-only behavior.

## Deliberate simplifications

- Collapse only duplicate `Sent` role entries in this change. Leave other special-role duplicates unchanged until there is a concrete report for them.
- Do not rename, delete, move, or merge remote IMAP folders. A logical view is reversible and avoids data loss.
- Do not add a separate background daemon or systemd service. The current in-process sync loop already satisfies background reception once the window is hidden instead of closed.
- Do not add startup-at-login behavior; that is a separate product decision.
- Do not deduplicate message rows by `Message-ID`; the first implementation groups navigation and counts while preserving the actual source folder on every envelope.

---

```

## Verbatim excerpt — acceptance criteria and boundary

```text
## Acceptance criteria

- One and only one visible `Sent` item is shown per account for the current `Sent` + `[Gmail]/Sent Mail` case.
- Its count is the sum of both physical folders, and selecting it loads both sources.
- Message actions continue to use each envelope's real source folder; no remote folder is deleted or renamed.
- Closing the foreground window does not stop IMAP sync, OAuth refresh, or new-mail notifications.
- The tray menu can restore/focus the UI and can explicitly quit the process.
- Linux behavior does not depend on unsupported tray click events.
- Existing UI, Rust, and build checks pass.

## Risks and follow-up boundary

- A logical view can still show the same RFC message twice if the server exposes duplicate physical copies. Do not add Message-ID deduplication in this pass; investigate only if the user still sees duplicate message rows after the navigation fix.
- If the requirement later expands to receiving mail after the user logs out, reboots, or kills Origami entirely, the single-process tray approach is insufficient; then plan a separate headless service/daemon and IPC bridge. That is not needed to keep the app alive after a normal window close.
```

## Status note

The plan's tasks are described in the past tense of an execution record; verify against the
current source rather than treating this page as proof of completion. Corresponding work is
visible in [[origami-app]] (tray lifecycle) and [[store-and-search]] (projection).

## Fed into

[[logical-vs-physical-message]] · [[origami-app]] · [[store-and-search]]
