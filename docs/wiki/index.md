---
title: Index
type: index
status: current
updated: 2026-09-27
---

# Wiki index

Catalog of every page. Read this first on a query, then drill into pages.
Categories: Meta · [[overview|Overview]] · Architecture · Decisions · Concepts · Entities ·
Status · Sources.

## Meta

- [[SCHEMA]] — conventions, source precedence, and the ingest / query / lint workflows.
- [[index]] — this catalog.
- [[log]] — append-only chronology of ingests, queries, and lint passes.

## Overview

- [[overview]] — current state of Origami: identity, stack, architecture, readiness.

## Architecture

- [[runtime-and-layers]] — process model, crate layers, threading, data flow.
- [[backend-seam]] — `MailBackend` / `SmtpSender` traits and protocol isolation.
- [[sync-engine]] — per-account actors, CONDSTORE/QRESYNC delta sync, IDLE, outbox replay, retry policy.
- [[store-and-search]] — SQLite/WAL schema, FTS5, unread-only paging, blob store, migrations, write integrity.
- [[display-cache-and-prefetch]] — LRU → SQLite → network open path, priority prefetch queue, budgets.
- [[message-model-and-threading]] — dual message identity, MIME, jwz threading.
- [[ui-state-and-rendering]] — Svelte state, unread-only list, sidebar tree, themes/palette, prefetch UI, HTML sanitization.
- [[accounts-and-secrets]] — config, keyring, onboarding wizard, OAuth2 PKCE and token refresh.

## Decisions

- [[adr-0001-app-owned-keys]] — every local entity gets an app-owned UUIDv7 key.
- [[adr-0002-backend-trait]] — protocol crates are isolated behind Origami traits.
- [[adr-0003-sync-algorithm]] — CONDSTORE-first, envelope-first sync.

## Concepts

- [[logical-vs-physical-message]] — one logical message, many physical sources; logical unread.
- [[condstore-qresync]] — the delta-sync machinery and its fallbacks.
- [[offline-outbox]] — queue offline mutations and replay on reconnect.
- [[content-addressed-store]] — raw RFC 822 bodies stored by content hash.

## Entities

- [[origami-core]] — the UI-agnostic Rust library crate.
- [[origami-app]] — the Tauri 2 shell, commands, tray, notifications.
- [[origami-cli]] — the thin debug CLI.
- [[ui-frontend]] — Svelte 5 + Vite + TypeScript frontend.
- [[protocol-dependencies]] — io-imap, io-smtp, and the pinned upstreams.

## Status

- [[release-readiness]] — classification, blockers, recommended order.
- [[build-and-verification]] — the commands and what was last verified.
- [[known-drift]] — places where plan and code disagree.

## Sources

- [[readme]] — repository README (layout and entry points).
- [[memory-md]] — the former `MEMORY.md` operational snapshot (now superseded).
- [[plan-md]] — master plan: locked decisions, milestones, risks.
- [[improvement-plan-md]] — P0–P7 daily-driver backlog.
- [[test-suite-md]] — Docker Dovecot/GreenMail integration harness.
- [[github-cicd-md]] — GitHub Actions pipeline, Arch package, release tags.
- [[hermes-sent-tray-background]] — plan: unified Sent projection + tray lifecycle.
- [[hermes-arch-local-release]] — plan: PKGBUILD and local pacman install.
- [[unread-only-list-filter]] — sticky unread-only thread list (store filter, keep-selected, chrome).
- [[sidebar-folders-tags-onboarding]] — nested folders, tag catalog, context menus, app-password wizard.
- [[hardening-caching-palette-spec]] — PR #1 spec: open-speed caching, hardening, palette (outcomes + gaps).
