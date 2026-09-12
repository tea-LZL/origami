---
title: Overview
type: overview
status: current
updated: 2026-09-12
sources:
  - README.md
  - docs/PLAN.md
  - docs/IMPROVEMENT_PLAN.md
  - crates/origami-core/src/lib.rs
---

# Origami — overview

Origami is a **provider-neutral, offline-first Linux desktop email client**. The product
target is Arch Linux and Wayland/Hyprland first, while the architecture stays portable.
Version `0.1.0`; workspace edition 2021, `rust-version` 1.85.

## Stack

| Concern | Choice |
|---|---|
| Core | Rust 2021 — `origami-core`, UI-agnostic |
| Desktop shell | Tauri 2 — `origami-app` |
| UI | Svelte 5 runes + TypeScript + Vite — `ui/` |
| Debug surface | Rust CLI — `origami-cli` |
| Persistence | SQLite/WAL + FTS5 + content-addressed blobs |
| Protocols | IMAP and SMTP behind Origami-owned traits |
| Packaging | Arch PKGBUILD via GitLab CI (signed pacman repo deferred) |

Tracked source was ~17,700 lines across 66 Rust/Svelte/TS/CSS files as of 2026-08-24
([[memory-md]]); the repository has since added the trap-focus and tag-catalog work.

## Architecture in one paragraph

The Svelte UI talks to `origami-app` over typed Tauri commands and events; `origami-app`
owns `AppState` (config + store + sync engine + cancellation) and `origami-core` owns the
SQLite store, the sync engine, and the protocol adapters. The UI reads normalized DTOs and
never blocks on IMAP: it reads only from SQLite while sync pushes deltas as events. There is
no daemon — sync, cache warming, outbox replay, notifications, and UI commands all run in the
existing process. See [[runtime-and-layers]].

## Messages have two identities

1. An **Origami-owned logical identity** (UUIDv7) used for deduplicated presentation.
2. Every **physical source** `(mailbox_id, server_uid)` for reads, flags, labels, moves,
   deletes, attachments, and sync.

Never treat an IMAP UID as globally unique, and never discard a retained physical source.
See [[logical-vs-physical-message]] and [[adr-0001-app-owned-keys]].

## What works today

The main daily-driver flows are implemented: multi-account onboarding with password and
Gmail/Microsoft OAuth and keyring-backed secrets; indexed envelope sync; SQLite/FTS search,
saved searches, unified Inbox, tags, conversations; body-on-demand plus bounded recent
display-cache warming; offline flag/move/delete/send with replay and an Outbox inspector;
reply/reply-all/forward, attachments, drafts, duplicate-send protection; logical cross-label
deduplication with all physical sources retained; chronological ordering by normalized
server-received time; isolated HTML rendering with DOMPurify, restrictive CSP, link
interception, and remote images blocked by default; themes, density, motion preferences,
resizable layouts, tray lifecycle, notifications, and keyboard list navigation.

Details: [[sync-engine]], [[store-and-search]], [[ui-state-and-rendering]],
[[backend-seam]], [[accounts-and-secrets]].

## Where it stands

**Feature-rich beta / daily-driver candidate — not public-production ready.** Public release
readiness lags feature breadth because proof and distribution gates are missing
([[release-readiness]]). Known plan-vs-code disagreements are tracked in [[known-drift]].

## Related

- [[plan-md]] — the master plan and milestone history.
- [[improvement-plan-md]] — the P0–P7 backlog.
- [[build-and-verification]] — how to build and test.
